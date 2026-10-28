use crate::db::DbConnection;
use crate::models::FolderMetadata;
use grammers_client::types::Peer;
use grammers_tl_types as tl;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudForum {
    pub id: i64,
    pub telegram_chat_id: i64,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudForumTopicFolder {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudForumTree {
    pub id: i64,
    pub telegram_chat_id: i64,
    pub name: String,
    pub is_default: bool,
    pub topics: Vec<TCloudForumTopicFolder>,
}

#[derive(Debug, Clone)]
pub struct TopicTarget {
    pub local_id: i64,
    pub forum_id: i64,
    pub chat_id: i64,
    pub access_hash: i64,
    pub topic_id: i32,
    pub name: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyTCloudChannel {
    pub channel_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForumMigrationResult {
    pub source_channel_id: i64,
    pub source_name: String,
    pub topic_folder_id: i64,
    pub migrated_messages: usize,
    pub skipped_messages: usize,
    pub source_deleted: bool,
}

pub fn is_topic_folder_id(id: i64) -> bool { id < 0 }
pub fn synthetic_folder_id(local_id: i64) -> i64 { -local_id.abs() }

pub fn topic_target(db: &DbConnection, folder_id: Option<i64>) -> Result<Option<TopicTarget>, String> {
    let Some(folder_id) = folder_id else { return Ok(None) };
    if folder_id >= 0 { return Ok(None) }
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut q = conn.prepare(
        "SELECT t.id,t.forum_id,f.telegram_chat_id,f.telegram_access_hash,t.telegram_topic_id,t.name
         FROM tcloud_forum_topics t JOIN tcloud_forums f ON f.id=t.forum_id
         WHERE t.id=?1 LIMIT 1"
    ).map_err(|e|e.to_string())?;
    q.bind((1, folder_id.abs())).map_err(|e|e.to_string())?;
    if let sqlite::State::Row=q.next().map_err(|e|e.to_string())? {
        return Ok(Some(TopicTarget {
            local_id:q.read(0).map_err(|e|e.to_string())?,
            forum_id:q.read(1).map_err(|e|e.to_string())?,
            chat_id:q.read(2).map_err(|e|e.to_string())?,
            access_hash:q.read(3).map_err(|e|e.to_string())?,
            topic_id:q.read::<i64,_>(4).map_err(|e|e.to_string())? as i32,
            name:q.read(5).map_err(|e|e.to_string())?,
        }))
    }
    Ok(None)
}

fn input_peer(chat_id:i64, access_hash:i64)->tl::enums::InputPeer {
    tl::enums::InputPeer::Channel(tl::types::InputPeerChannel{channel_id:chat_id,access_hash})
}

fn random_id()->i64 {
    use std::time::{SystemTime,UNIX_EPOCH};
    let n=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    (n as u64 ^ ((n>>64) as u64)).wrapping_mul(0x9E3779B97F4A7C15) as i64
}

fn forums(conn:&sqlite::Connection)->Result<Vec<TCloudForum>,String>{
    let mut q=conn.prepare("SELECT id,telegram_chat_id,name,is_default FROM tcloud_forums ORDER BY name COLLATE NOCASE").map_err(|e|e.to_string())?;
    let mut out=Vec::new();
    while let sqlite::State::Row=q.next().map_err(|e|e.to_string())? {
        out.push(TCloudForum{id:q.read(0).map_err(|e|e.to_string())?,telegram_chat_id:q.read(1).map_err(|e|e.to_string())?,name:q.read(2).map_err(|e|e.to_string())?,is_default:q.read::<i64,_>(3).map_err(|e|e.to_string())?!=0});
    }
    Ok(out)
}

async fn resolve_forum_peer(client:&grammers_client::Client, chat_id:i64, cache:&Arc<tokio::sync::RwLock<HashMap<i64,Peer>>>)->Result<Peer,String>{
    if let Some(p)=cache.read().await.get(&chat_id).cloned(){return Ok(p)}
    let mut dialogs=client.iter_dialogs();
    while let Some(d)=dialogs.next().await.map_err(|e|e.to_string())? {
        let p = d.peer().clone();
        let is_target = match &p {
            Peer::Channel(channel) => channel.raw.id == chat_id,
            _ => false,
        };
        if is_target {
            cache.write().await.insert(chat_id, p.clone());
            return Ok(p);
        }
    }
    Err("Fórum do TCloud não encontrado no Telegram".into())
}

async fn create_forum(client:&grammers_client::Client,db:&DbConnection,cache:&Arc<tokio::sync::RwLock<HashMap<i64,Peer>>>,name:&str,make_default:bool)->Result<TCloudForum,String>{
    let title=name.trim(); if title.is_empty(){return Err("Nome da pasta vazio".into())}
    let updates=client.invoke(&tl::functions::channels::CreateChannel{broadcast:false,megagroup:true,for_import:false,forum:true,title:title.into(),about:"TCloud storage".into(),geo_point:None,address:None,ttl_period:None}).await.map_err(|e|e.to_string())?;
    let chats=match updates { tl::enums::Updates::Combined(v)=>v.chats, tl::enums::Updates::Updates(v)=>v.chats, _=>Vec::new() };
    let channel=chats.into_iter().find_map(|c|match c{tl::enums::Chat::Channel(ch)=>Some(ch),_=>None}).ok_or_else(||"A pasta não pôde ser criada agora".to_string())?;
    let chat_id=channel.id; let access_hash=channel.access_hash.unwrap_or(0);

    // CreateChannel already returned the complete channel object. Cache it
    // immediately instead of waiting for Telegram dialogs to refresh.
    cache.write().await.insert(
        chat_id,
        Peer::Channel(grammers_client::types::Channel { raw: channel.clone() }),
    );

    let now=chrono::Utc::now().timestamp();
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    if make_default { conn.execute("UPDATE tcloud_forums SET is_default=0").map_err(|e|e.to_string())?; }
    let mut q=conn.prepare("INSERT INTO tcloud_forums(telegram_chat_id,telegram_access_hash,name,is_default,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)").map_err(|e|e.to_string())?;
    q.bind((1,chat_id)).map_err(|e|e.to_string())?; q.bind((2,access_hash)).map_err(|e|e.to_string())?; q.bind((3,title)).map_err(|e|e.to_string())?; q.bind((4,if make_default{1}else{0})).map_err(|e|e.to_string())?; q.bind((5,now)).map_err(|e|e.to_string())?; q.bind((6,now)).map_err(|e|e.to_string())?; q.next().map_err(|e|e.to_string())?;
    Ok(TCloudForum{id:crate::db::last_insert_rowid(&conn)?,telegram_chat_id:chat_id,name:title.into(),is_default:make_default})
}

fn unique_main_folder_name(db: &DbConnection, requested: &str) -> Result<String, String> {
    let base = requested.trim();
    if base.is_empty() { return Err("Nome da pasta vazio".to_string()); }
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut query = conn.prepare("SELECT name FROM tcloud_forums ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let mut used = HashSet::<String>::new();
    while let sqlite::State::Row = query.next().map_err(|e| e.to_string())? {
        used.insert(query.read::<String,_>(0).map_err(|e| e.to_string())?.trim().to_lowercase());
    }
    if !used.contains(&base.to_lowercase()) { return Ok(base.to_string()); }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{} ({})", base, suffix);
        if !used.contains(&candidate.to_lowercase()) { return Ok(candidate); }
        suffix += 1;
    }
}

fn unique_topic_folder_name(db: &DbConnection, forum_id: i64, requested: &str) -> Result<String, String> {
    let base = requested.trim();
    if base.is_empty() { return Err("Nome da pasta vazio".to_string()); }
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut query = conn.prepare("SELECT name FROM tcloud_forum_topics WHERE forum_id=?1 ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    query.bind((1, forum_id)).map_err(|e| e.to_string())?;
    let mut used = HashSet::<String>::new();
    while let sqlite::State::Row = query.next().map_err(|e| e.to_string())? {
        used.insert(query.read::<String,_>(0).map_err(|e| e.to_string())?.trim().to_lowercase());
    }
    if !used.contains(&base.to_lowercase()) { return Ok(base.to_string()); }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{} ({})", base, suffix);
        if !used.contains(&candidate.to_lowercase()) { return Ok(candidate); }
        suffix += 1;
    }
}

fn main_folder_name_conflicts(db: &DbConnection, forum_id: i64, name: &str) -> Result<bool, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut query = conn.prepare("SELECT id FROM tcloud_forums WHERE id<>?1 AND lower(name)=lower(?2) LIMIT 1")
        .map_err(|e| e.to_string())?;
    query.bind((1, forum_id)).map_err(|e| e.to_string())?;
    query.bind((2, name.trim())).map_err(|e| e.to_string())?;
    Ok(matches!(query.next().map_err(|e| e.to_string())?, sqlite::State::Row))
}

fn topic_name_conflicts(db: &DbConnection, forum_id: i64, local_id: i64, name: &str) -> Result<bool, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut query = conn.prepare("SELECT id FROM tcloud_forum_topics WHERE forum_id=?1 AND id<>?2 AND lower(name)=lower(?3) LIMIT 1")
        .map_err(|e| e.to_string())?;
    query.bind((1, forum_id)).map_err(|e| e.to_string())?;
    query.bind((2, local_id)).map_err(|e| e.to_string())?;
    query.bind((3, name.trim())).map_err(|e| e.to_string())?;
    Ok(matches!(query.next().map_err(|e| e.to_string())?, sqlite::State::Row))
}

#[derive(Debug, Clone)]
struct RemoteForumTopic {
    telegram_topic_id: i32,
    name: String,
}

async fn sync_forum_topics_from_telegram(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
    forum_id: i64,
    chat_id: i64,
    access_hash: i64,
    forum_peer: &Peer,
) -> Result<usize, String> {
    let peer = input_peer(chat_id, access_hash);
    let mut offset_date = 0;
    let mut offset_id = 0;
    let mut offset_topic = 0;
    let mut seen_topic_ids = HashSet::<i32>::new();
    let mut topics = HashMap::<i32, RemoteForumTopic>::new();
    let mut expected_total: Option<usize> = None;
    let mut complete = false;

    for _ in 0..100 {
        let result = client.invoke(&tl::functions::messages::GetForumTopics {
            peer: peer.clone(),
            q: None,
            offset_date,
            offset_id,
            offset_topic,
            limit: 100,
        }).await.map_err(|e| e.to_string())?;

        let page = tl::types::messages::ForumTopics::try_from(result)
            .map_err(|_| "Telegram retornou uma lista de pastas inesperada".to_string())?;
        expected_total.get_or_insert(page.count.max(0) as usize);

        let mut next_offset: Option<(i32, i32, i32)> = None;
        let before = seen_topic_ids.len();
        for topic in page.topics {
            if let tl::enums::ForumTopic::Topic(value) = topic {
                seen_topic_ids.insert(value.id);
                next_offset = Some((value.date, value.top_message, value.id));

                if value.id != 1 {
                    topics.insert(value.id, RemoteForumTopic {
                        telegram_topic_id: value.id,
                        name: value.title.trim().to_string(),
                    });
                }
            }
        }

        if expected_total.unwrap_or(0) == 0
            || seen_topic_ids.len() >= expected_total.unwrap_or(0)
        {
            complete = true;
            break;
        }

        let Some((next_date, next_id, next_topic)) = next_offset else {
            break;
        };
        if seen_topic_ids.len() == before
            || (next_date == offset_date && next_id == offset_id && next_topic == offset_topic)
        {
            break;
        }
        offset_date = next_date;
        offset_id = next_id;
        offset_topic = next_topic;
    }

    let now = chrono::Utc::now().timestamp();
    let mut local_topic_ids = Vec::<i64>::new();
    {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

        for remote in topics.values() {
            let clean_name = if remote.name.is_empty() {
                format!("Pasta {}", remote.telegram_topic_id)
            } else {
                remote.name.clone()
            };

            let mut upsert = conn.prepare(
                "INSERT INTO tcloud_forum_topics(forum_id,telegram_topic_id,name,created_at,updated_at)\n                 VALUES(?1,?2,?3,?4,?5)\n                 ON CONFLICT(forum_id,telegram_topic_id) DO UPDATE SET\n                    name=excluded.name,\n                    updated_at=excluded.updated_at"
            ).map_err(|e| e.to_string())?;
            upsert.bind((1, forum_id)).map_err(|e| e.to_string())?;
            upsert.bind((2, i64::from(remote.telegram_topic_id))).map_err(|e| e.to_string())?;
            upsert.bind((3, clean_name.as_str())).map_err(|e| e.to_string())?;
            upsert.bind((4, now)).map_err(|e| e.to_string())?;
            upsert.bind((5, now)).map_err(|e| e.to_string())?;
            upsert.next().map_err(|e| e.to_string())?;

            let mut find = conn.prepare(
                "SELECT id FROM tcloud_forum_topics WHERE forum_id=?1 AND telegram_topic_id=?2 LIMIT 1"
            ).map_err(|e| e.to_string())?;
            find.bind((1, forum_id)).map_err(|e| e.to_string())?;
            find.bind((2, i64::from(remote.telegram_topic_id))).map_err(|e| e.to_string())?;
            if let sqlite::State::Row = find.next().map_err(|e| e.to_string())? {
                let local_id = find.read::<i64, _>(0).map_err(|e| e.to_string())?;
                local_topic_ids.push(local_id);

                let folder_id = synthetic_folder_id(local_id);
                let mut metadata = conn.prepare(
                    "INSERT INTO folder_metadata(channel_id,name,username,is_public,display_order,group_id)\n                     VALUES(?1,?2,NULL,0,(SELECT COALESCE(MAX(display_order),0)+1 FROM folder_metadata),NULL)\n                     ON CONFLICT(channel_id) DO UPDATE SET name=excluded.name"
                ).map_err(|e| e.to_string())?;
                metadata.bind((1, folder_id)).map_err(|e| e.to_string())?;
                metadata.bind((2, clean_name.as_str())).map_err(|e| e.to_string())?;
                metadata.next().map_err(|e| e.to_string())?;
            }
        }

        if complete {
            let mut existing = conn.prepare(
                "SELECT id,telegram_topic_id FROM tcloud_forum_topics WHERE forum_id=?1"
            ).map_err(|e| e.to_string())?;
            existing.bind((1, forum_id)).map_err(|e| e.to_string())?;
            let mut stale = Vec::<i64>::new();
            while let sqlite::State::Row = existing.next().map_err(|e| e.to_string())? {
                let local_id = existing.read::<i64, _>(0).map_err(|e| e.to_string())?;
                let telegram_topic_id = existing.read::<i64, _>(1).map_err(|e| e.to_string())? as i32;
                if !seen_topic_ids.contains(&telegram_topic_id) {
                    stale.push(local_id);
                }
            }
            drop(existing);

            for local_id in stale {
                let folder_id = synthetic_folder_id(local_id);
                let mut delete_metadata = conn.prepare(
                    "DELETE FROM folder_metadata WHERE channel_id=?1"
                ).map_err(|e| e.to_string())?;
                delete_metadata.bind((1, folder_id)).map_err(|e| e.to_string())?;
                delete_metadata.next().map_err(|e| e.to_string())?;

                let mut delete_topic = conn.prepare(
                    "DELETE FROM tcloud_forum_topics WHERE id=?1"
                ).map_err(|e| e.to_string())?;
                delete_topic.bind((1, local_id)).map_err(|e| e.to_string())?;
                delete_topic.next().map_err(|e| e.to_string())?;
            }
        }
    }

    {
        let mut write = cache.write().await;
        write.insert(chat_id, forum_peer.clone());
        for local_id in local_topic_ids {
            write.insert(synthetic_folder_id(local_id), forum_peer.clone());
        }
    }

    Ok(topics.len())
}

async fn sync_forum_catalog_from_telegram(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
) -> Result<usize, String> {
    let mut dialogs = client.iter_dialogs();
    let mut discovered = Vec::<(i64, i64, String, Peer)>::new();

    while let Some(dialog) = dialogs.next().await.map_err(|e| e.to_string())? {
        if let Peer::Channel(channel) = &dialog.peer {
            if !channel.raw.forum {
                continue;
            }
            let access_hash = channel.raw.access_hash.unwrap_or(0);
            discovered.push((
                channel.raw.id,
                access_hash,
                channel.raw.title.trim().to_string(),
                dialog.peer.clone(),
            ));
        }
    }

    let now = chrono::Utc::now().timestamp();
    let mut local_forums = Vec::<(i64, i64, i64, Peer)>::new();
    {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        for (chat_id, access_hash, title, peer) in discovered {
            let clean_title = if title.is_empty() {
                format!("Pasta {}", chat_id)
            } else {
                title
            };
            let mut upsert = conn.prepare(
                "INSERT INTO tcloud_forums(telegram_chat_id,telegram_access_hash,name,is_default,created_at,updated_at)\n                 VALUES(?1,?2,?3,0,?4,?5)\n                 ON CONFLICT(telegram_chat_id) DO UPDATE SET\n                    telegram_access_hash=excluded.telegram_access_hash,\n                    name=excluded.name,\n                    updated_at=excluded.updated_at"
            ).map_err(|e| e.to_string())?;
            upsert.bind((1, chat_id)).map_err(|e| e.to_string())?;
            upsert.bind((2, access_hash)).map_err(|e| e.to_string())?;
            upsert.bind((3, clean_title.as_str())).map_err(|e| e.to_string())?;
            upsert.bind((4, now)).map_err(|e| e.to_string())?;
            upsert.bind((5, now)).map_err(|e| e.to_string())?;
            upsert.next().map_err(|e| e.to_string())?;

            let mut find = conn.prepare(
                "SELECT id FROM tcloud_forums WHERE telegram_chat_id=?1 LIMIT 1"
            ).map_err(|e| e.to_string())?;
            find.bind((1, chat_id)).map_err(|e| e.to_string())?;
            if let sqlite::State::Row = find.next().map_err(|e| e.to_string())? {
                local_forums.push((
                    find.read::<i64, _>(0).map_err(|e| e.to_string())?,
                    chat_id,
                    access_hash,
                    peer,
                ));
            }
        }
    }

    let mut synced = 0usize;
    for (forum_id, chat_id, access_hash, peer) in &local_forums {
        if *access_hash == 0 {
            log::warn!("TCloud: pasta principal {} sem access_hash; subpastas serão atualizadas na próxima resolução.", chat_id);
            continue;
        }
        match sync_forum_topics_from_telegram(
            client,
            db,
            cache,
            *forum_id,
            *chat_id,
            *access_hash,
            peer,
        ).await {
            Ok(_) => synced += 1,
            Err(error) => log::warn!(
                "TCloud: não foi possível atualizar as subpastas de {}: {}",
                chat_id,
                error
            ),
        }
    }

    attach_topic_peer_aliases(db, cache).await?;
    Ok(synced)
}

async fn forum_is_accessible(
    client: &grammers_client::Client,
    db: &DbConnection,
    forum: &TCloudForum,
) -> bool {
    let row = forum_row(db, forum.id);
    let Ok((chat_id, access_hash, _)) = row else {
        return false;
    };

    let peer = input_peer(chat_id, access_hash);
    client.invoke(&tl::functions::messages::GetForumTopics {
        peer,
        q: None,
        offset_date: 0,
        offset_id: 0,
        offset_topic: 0,
        limit: 1,
    }).await.is_ok()
}

fn remove_stale_forum_local(
    db: &DbConnection,
    forum_id: i64,
) -> Result<Vec<i64>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    // Collect synthetic folder IDs first so their folder_metadata rows can be
    // removed without touching unrelated legacy channels/groups.
    let mut topic_query = conn.prepare(
        "SELECT id FROM tcloud_forum_topics WHERE forum_id=?1"
    ).map_err(|e| e.to_string())?;
    topic_query.bind((1, forum_id)).map_err(|e| e.to_string())?;

    let mut synthetic_ids = Vec::new();
    while let sqlite::State::Row = topic_query.next().map_err(|e| e.to_string())? {
        let local_id = topic_query.read::<i64, _>(0).map_err(|e| e.to_string())?;
        synthetic_ids.push(synthetic_folder_id(local_id));
    }
    drop(topic_query);

    for folder_id in &synthetic_ids {
        let mut delete_meta = conn.prepare(
            "DELETE FROM folder_metadata WHERE channel_id=?1"
        ).map_err(|e| e.to_string())?;
        delete_meta.bind((1, *folder_id)).map_err(|e| e.to_string())?;
        delete_meta.next().map_err(|e| e.to_string())?;
    }

    let mut delete_topics = conn.prepare(
        "DELETE FROM tcloud_forum_topics WHERE forum_id=?1"
    ).map_err(|e| e.to_string())?;
    delete_topics.bind((1, forum_id)).map_err(|e| e.to_string())?;
    delete_topics.next().map_err(|e| e.to_string())?;

    let mut delete_forum = conn.prepare(
        "DELETE FROM tcloud_forums WHERE id=?1"
    ).map_err(|e| e.to_string())?;
    delete_forum.bind((1, forum_id)).map_err(|e| e.to_string())?;
    delete_forum.next().map_err(|e| e.to_string())?;

    Ok(synthetic_ids)
}

async fn discard_stale_forum(
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
    forum: &TCloudForum,
) -> Result<(), String> {
    let stale_folder_ids = remove_stale_forum_local(db, forum.id)?;
    let mut cache_write = cache.write().await;
    cache_write.remove(&forum.telegram_chat_id);
    for folder_id in stale_folder_ids {
        cache_write.remove(&folder_id);
    }
    Ok(())
}

async fn default_forum(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
) -> Result<TCloudForum, String> {
    let existing = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        forums(&conn)?
    };

    // Prefer the configured default, but validate it against Telegram instead
    // of trusting a stale local database row.
    if let Some(default) = existing.iter().find(|forum| forum.is_default).cloned() {
        if forum_is_accessible(client, db, &default).await {
            // Cache resolution is best-effort; raw InputPeer(id + access_hash)
            // remains enough for topic API calls.
            let _ = resolve_forum_peer(client, default.telegram_chat_id, cache).await;
            return Ok(default);
        }

        log::warn!(
            "TCloud default forum {} ({}) is stale; recreating it.",
            default.name,
            default.telegram_chat_id
        );
        discard_stale_forum(db, cache, &default).await?;
    }

    // A non-default registered forum can be promoted only if Telegram confirms
    // that it still exists and is accessible.
    let remaining = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        forums(&conn)?
    };

    for candidate in remaining {
        if !forum_is_accessible(client, db, &candidate).await {
            log::warn!(
                "Removing stale TCloud forum registration {} ({})",
                candidate.name,
                candidate.telegram_chat_id
            );
            discard_stale_forum(db, cache, &candidate).await?;
            continue;
        }

        {
            let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
            conn.execute("UPDATE tcloud_forums SET is_default=0")
                .map_err(|e| e.to_string())?;
            let mut query = conn.prepare(
                "UPDATE tcloud_forums SET is_default=1,updated_at=?1 WHERE id=?2"
            ).map_err(|e| e.to_string())?;
            query.bind((1, chrono::Utc::now().timestamp())).map_err(|e| e.to_string())?;
            query.bind((2, candidate.id)).map_err(|e| e.to_string())?;
            query.next().map_err(|e| e.to_string())?;
        }

        let promoted = TCloudForum {
            is_default: true,
            ..candidate
        };
        let _ = resolve_forum_peer(client, promoted.telegram_chat_id, cache).await;
        return Ok(promoted);
    }

    // Nothing valid remains: create the real default forum automatically.
    create_forum(client, db, cache, "TCloud", true).await
}

fn forum_row(db:&DbConnection,id:i64)->Result<(i64,i64,String),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;let mut q=conn.prepare("SELECT telegram_chat_id,telegram_access_hash,name FROM tcloud_forums WHERE id=?1").map_err(|e|e.to_string())?;q.bind((1,id)).map_err(|e|e.to_string())?;
    if let sqlite::State::Row=q.next().map_err(|e|e.to_string())? { return Ok((q.read(0).map_err(|e|e.to_string())?,q.read(1).map_err(|e|e.to_string())?,q.read(2).map_err(|e|e.to_string())?)) }
    Err("Pasta principal não encontrada".into())
}

pub async fn create_topic_internal(client:&grammers_client::Client,db:&DbConnection,cache:&Arc<tokio::sync::RwLock<HashMap<i64,Peer>>>,name:&str,forum_id:Option<i64>)->Result<FolderMetadata,String>{
    let forum=if let Some(id)=forum_id { let (chat,_,n)=forum_row(db,id)?; TCloudForum{id,telegram_chat_id:chat,name:n,is_default:false} } else { default_forum(client,db,cache).await? };
    let title=unique_topic_folder_name(db,forum.id,name)?;
    let (chat_id,access_hash,_)=forum_row(db,forum.id)?;

    // Persisted id/access_hash is enough for topic creation. Avoid walking all
    // Telegram dialogs here because that made New folder unnecessarily slow.
    let peer=input_peer(chat_id,access_hash);
    client.invoke(&tl::functions::messages::CreateForumTopic{title_missing:false,peer:peer.clone(),title:title.clone(),icon_color:Some(0x6FB9F0),icon_emoji_id:None,random_id:random_id(),send_as:None}).await.map_err(|e|e.to_string())?;
    let result = client.invoke(&tl::functions::messages::GetForumTopics {
        peer,
        q: Some(title.clone()),
        offset_date: 0,
        offset_id: 0,
        offset_topic: 0,
        limit: 20,
    }).await.map_err(|e| e.to_string())?;

    let forum_topics = tl::types::messages::ForumTopics::try_from(result)
        .map_err(|_| "Não foi possível atualizar a lista de pastas agora".to_string())?;
    let topic_id = forum_topics.topics
        .into_iter()
        .filter_map(|topic| match topic {
            tl::enums::ForumTopic::Topic(value) => Some(value),
            _ => None,
        })
        .filter(|topic| topic.title == title)
        .max_by_key(|topic| topic.date)
        .map(|topic| topic.id)
        .ok_or_else(|| "A pasta foi criada, mas ainda não apareceu na lista. Tente atualizar.".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let local_id={let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;let mut q=conn.prepare("INSERT INTO tcloud_forum_topics(forum_id,telegram_topic_id,name,created_at,updated_at) VALUES(?1,?2,?3,?4,?5)").map_err(|e|e.to_string())?;q.bind((1,forum.id)).map_err(|e|e.to_string())?;q.bind((2,i64::from(topic_id))).map_err(|e|e.to_string())?;q.bind((3,title.as_str())).map_err(|e|e.to_string())?;q.bind((4,now)).map_err(|e|e.to_string())?;q.bind((5,now)).map_err(|e|e.to_string())?;q.next().map_err(|e|e.to_string())?;crate::db::last_insert_rowid(&conn)?};
    let folder_id=synthetic_folder_id(local_id);
    {let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;let mut q=conn.prepare("INSERT OR REPLACE INTO folder_metadata(channel_id,name,username,is_public,display_order,group_id) VALUES(?1,?2,NULL,0,(SELECT COALESCE(MAX(display_order),0)+1 FROM folder_metadata),NULL)").map_err(|e|e.to_string())?;q.bind((1,folder_id)).map_err(|e|e.to_string())?;q.bind((2,title.as_str())).map_err(|e|e.to_string())?;q.next().map_err(|e|e.to_string())?;}
    if let Some(real)=cache.read().await.get(&chat_id).cloned(){cache.write().await.insert(folder_id,real);}
    Ok(FolderMetadata{id:folder_id,parent_id:None,name:title.clone(),username:None,is_public:false,group_id:None,display_order:0})
}

pub async fn attach_topic_peer_aliases(db:&DbConnection,cache:&Arc<tokio::sync::RwLock<HashMap<i64,Peer>>>)->Result<(),String>{
    let rows={let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;let mut q=conn.prepare("SELECT t.id,f.telegram_chat_id FROM tcloud_forum_topics t JOIN tcloud_forums f ON f.id=t.forum_id").map_err(|e|e.to_string())?;let mut v=Vec::new();while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{v.push((synthetic_folder_id(q.read::<i64,_>(0).map_err(|e|e.to_string())?),q.read::<i64,_>(1).map_err(|e|e.to_string())?));}v};
    let snapshot=cache.read().await.clone();let mut w=cache.write().await;for (folder,chat) in rows{if let Some(p)=snapshot.get(&chat).cloned(){w.insert(folder,p);}}Ok(())
}

pub fn forum_tree(conn:&sqlite::Connection)->Result<Vec<TCloudForumTree>,String>{
    let all_forums = forums(conn)?;
    let mut output = Vec::new();

    for forum in all_forums {
        let mut topic_query = conn.prepare(
            "SELECT t.id, COALESCE(m.name, t.name) AS topic_name
             FROM tcloud_forum_topics t
             LEFT JOIN folder_metadata m ON m.channel_id = -t.id
             WHERE t.forum_id=?1
             ORDER BY COALESCE(m.display_order, 0) ASC, topic_name COLLATE NOCASE ASC"
        ).map_err(|e|e.to_string())?;
        topic_query.bind((1, forum.id)).map_err(|e|e.to_string())?;

        let mut topics = Vec::new();
        while let sqlite::State::Row = topic_query.next().map_err(|e|e.to_string())? {
            let local_topic_id = topic_query.read::<i64,_>(0).map_err(|e|e.to_string())?;
            let topic_name = topic_query.read::<String,_>(1).map_err(|e|e.to_string())?;
            topics.push(TCloudForumTopicFolder {
                id: synthetic_folder_id(local_topic_id),
                name: topic_name,
            });
        }

        output.push(TCloudForumTree {
            id: forum.id,
            telegram_chat_id: forum.telegram_chat_id,
            name: forum.name,
            is_default: forum.is_default,
            topics,
        });
    }

    Ok(output)
}

#[tauri::command]
pub async fn tcloud_forum_ensure_default(
    app:AppHandle,
    db:State<'_,DbConnection>,
    state:State<'_,crate::commands::TelegramState>,
)->Result<TCloudForum,String>{
    let client=state.client.lock().await.clone().ok_or_else(||"Telegram não conectado".to_string())?;
    let forum=default_forum(&client,&db,&state.peer_cache).await?;
    // 14.26: se o TCloud precisar criar/restaurar a pasta padr?o,
    // projete-a imediatamente no Explorer.
    if let Err(error) =
        crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await
    {
        log::warn!(
            "TCloud Explorer: pasta padr?o atualizada, mas refresh imediato falhou: {}",
            error
        );
        crate::commands::tcloud_cfapi::schedule_folder_namespace_refresh(app);
    }
    Ok(forum)
}
#[tauri::command]
pub async fn tcloud_forum_create(
    name:String,
    make_default:bool,
    app:AppHandle,
    db:State<'_,DbConnection>,
    state:State<'_,crate::commands::TelegramState>,
)->Result<TCloudForum,String>{
    let started=std::time::Instant::now();
    let client=state.client.lock().await.clone().ok_or_else(||"TCloud está sem conexão. Tente novamente quando a internet voltar.".to_string())?;
    let unique_name=unique_main_folder_name(&db,&name)?;
    let forum=create_forum(&client,&db,&state.peer_cache,&unique_name,make_default).await?;
    // 14.26: a pasta deve aparecer no Explorer assim que o Telegram
    // confirmar a cria??o, sem esperar o debounce do reconciliador.
    if let Err(error) =
        crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await
    {
        log::warn!(
            "TCloud Explorer: pasta principal criada, mas atualiza??o local foi adiada: {}",
            error
        );
        crate::commands::tcloud_cfapi::schedule_folder_namespace_refresh(app.clone());
    }
    let _ = app.emit("tcloud-folder-tree-changed", ());
    log::info!("TCloud: pasta principal criada em {} ms", started.elapsed().as_millis());
    Ok(forum)
}
#[tauri::command] pub fn tcloud_forum_list(db:State<'_,DbConnection>)->Result<Vec<TCloudForum>,String>{let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;forums(&conn)}
#[tauri::command]
pub fn tcloud_forum_list_tree_cached(db:State<'_,DbConnection>)->Result<Vec<TCloudForumTree>,String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    forum_tree(&conn)
}
#[tauri::command]
pub async fn tcloud_forum_list_tree(
    app: AppHandle,
    db: State<'_, DbConnection>,
    state: State<'_, crate::commands::TelegramState>,
) -> Result<Vec<TCloudForumTree>, String> {
    // The visible folder tree is refreshed from Telegram itself. This imports
    // forums created outside TCloud as well as topic renames/deletions, so the
    // sidebar and Explorer no longer depend on a stale local-only catalog.
    if let Some(client) = state.client.lock().await.clone() {
        match sync_forum_catalog_from_telegram(&client, &db, &state.peer_cache).await {
            Ok(_) => crate::commands::tcloud_cfapi::schedule_folder_namespace_refresh(app.clone()),
            Err(error) => log::warn!("TCloud: não foi possível atualizar todas as pastas do Telegram: {}", error),
        }

        // sync_forum_catalog_from_telegram already resolves the forum peers and
        // attaches topic aliases. Avoid a second full dialog walk here.
    }
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    forum_tree(&conn)
}
#[tauri::command] pub fn tcloud_forum_set_default(forum_id:i64,db:State<'_,DbConnection>)->Result<(),String>{let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;conn.execute("UPDATE tcloud_forums SET is_default=0").map_err(|e|e.to_string())?;let mut q=conn.prepare("UPDATE tcloud_forums SET is_default=1,updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;q.bind((1,chrono::Utc::now().timestamp())).map_err(|e|e.to_string())?;q.bind((2,forum_id)).map_err(|e|e.to_string())?;q.next().map_err(|e|e.to_string())?;Ok(())}

#[tauri::command]
pub async fn tcloud_forum_rename(
    forum_id: i64,
    new_name: String,
    app: AppHandle,
    db: State<'_, DbConnection>,
    state: State<'_, crate::commands::TelegramState>,
) -> Result<(), String> {
    let client = state.client.lock().await.clone()
        .ok_or_else(|| "TCloud está sem conexão. Tente novamente quando a internet voltar.".to_string())?;
    rename_forum_internal(&client, &db, &state.peer_cache, forum_id, &new_name).await?;
    if let Err(error) = crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await {
        log::warn!("TCloud Explorer: pasta principal renomeada, mas atualização local foi adiada: {}", error);
    }
    let _ = app.emit("tcloud-folder-tree-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn tcloud_forum_delete_empty(
    forum_id: i64,
    app: AppHandle,
    db: State<'_, DbConnection>,
    state: State<'_, crate::commands::TelegramState>,
) -> Result<(), String> {
    let client = state.client.lock().await.clone()
        .ok_or_else(|| "TCloud está sem conexão. Tente novamente quando a internet voltar.".to_string())?;
    delete_empty_forum_internal(&client, &db, &state.peer_cache, forum_id).await?;
    if let Err(error) = crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await {
        log::warn!("TCloud Explorer: pasta principal removida, mas atualização local foi adiada: {}", error);
    }
    let _ = app.emit("tcloud-folder-tree-changed", ());
    Ok(())
}
#[tauri::command]
pub async fn tcloud_forum_create_topic(
    name:String,
    forum_id:Option<i64>,
    app:AppHandle,
    db:State<'_,DbConnection>,
    state:State<'_,crate::commands::TelegramState>,
)->Result<FolderMetadata,String>{
    let started=std::time::Instant::now();
    let client=state.client.lock().await.clone().ok_or_else(||"Telegram não conectado".to_string())?;
    let folder=create_topic_internal(&client,&db,&state.peer_cache,&name,forum_id).await?;
    // 14.26: subpastas novas tamb?m entram no namespace do Explorer
    // imediatamente. O agendador permanece apenas como fallback.
    if let Err(error) =
        crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await
    {
        log::warn!(
            "TCloud Explorer: subpasta criada, mas atualiza??o local foi adiada: {}",
            error
        );
        crate::commands::tcloud_cfapi::schedule_folder_namespace_refresh(app.clone());
    }
    let _ = app.emit("tcloud-folder-tree-changed", ());
    log::info!("TCloud: subpasta criada em {} ms", started.elapsed().as_millis());
    Ok(folder)
}


fn clean_legacy_channel_name(name: &str) -> String {
    let trimmed = name.trim();
    for prefix in ["TCloud -", "TCloud –", "TCloud —"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let clean = rest.trim();
            if !clean.is_empty() {
                return clean.to_string();
            }
        }
    }
    trimmed
        .replace(" [TD]", "")
        .replace(" [td]", "")
        .replace("[TD]", "")
        .replace("[td]", "")
        .trim()
        .to_string()
}

#[tauri::command]
pub fn tcloud_forum_list_legacy_channels(
    db: State<'_, DbConnection>,
) -> Result<Vec<LegacyTCloudChannel>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut stmt = conn.prepare(
        "SELECT channel_id, name
         FROM folder_metadata
         WHERE channel_id > 0
         ORDER BY display_order ASC, name COLLATE NOCASE"
    ).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let channel_id = stmt.read::<i64, _>(0).map_err(|e| e.to_string())?;
        let name = stmt.read::<String, _>(1).map_err(|e| e.to_string())?;
        out.push(LegacyTCloudChannel {
            channel_id,
            name: clean_legacy_channel_name(&name),
        });
    }
    Ok(out)
}

async fn download_message_media_to_temp(
    client: &grammers_client::Client,
    message: &grammers_client::types::Message,
    temp_dir: &std::path::Path,
) -> Result<Option<std::path::PathBuf>, String> {
    let Some(media) = message.media() else {
        return Ok(None);
    };

    let file_name = match &media {
        grammers_client::types::Media::Document(document) => {
            let value = document.name();
            if value.trim().is_empty() {
                format!("message_{}.bin", message.id())
            } else {
                value.to_string()
            }
        }
        grammers_client::types::Media::Photo(_) => format!("photo_{}.jpg", message.id()),
        _ => format!("media_{}.bin", message.id()),
    };

    let safe_name = file_name.replace(['/', '\\'], "_");
    let path = temp_dir.join(format!("{}_{}", message.id(), safe_name));
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|error| error.to_string())?;

    let mut download = client.iter_download(&media);
    while let Some(chunk) = download.next().await.map_err(|error| error.to_string())? {
        file.write_all(&chunk).await.map_err(|error| error.to_string())?;
    }
    file.flush().await.map_err(|error| error.to_string())?;
    Ok(Some(path))
}

pub async fn copy_messages_to_topic_internal(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
    source_peer: &Peer,
    message_ids: &[i32],
    target_folder_id: i64,
    app: &tauri::AppHandle,
) -> Result<Vec<i32>, String> {
    let topic = topic_target(db, Some(target_folder_id))?
        .ok_or_else(|| "A pasta de destino não é um tópico do TCloud".to_string())?;
    let destination_peer = match cache.read().await.get(&topic.chat_id).cloned() {
        Some(peer) => peer,
        None => resolve_forum_peer(client, topic.chat_id, cache).await?,
    };
    let temp_dir = app.path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("topic-copy");
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|error| error.to_string())?;

    let messages = client.get_messages_by_id(source_peer, message_ids)
        .await
        .map_err(|error| format!("Falha ao ler mensagens de origem: {error}"))?;
    let mut created = Vec::new();
    for message in messages.into_iter().flatten() {
        let text = message.text().trim().to_string();
        let temp_media = download_message_media_to_temp(client, &message, &temp_dir).await?;
        let sent = if let Some(path) = temp_media.as_ref() {
            let mut file = tokio::fs::File::open(path).await.map_err(|error| error.to_string())?;
            let size = file.metadata().await.map_err(|error| error.to_string())?.len() as usize;
            let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("arquivo.bin").to_string();
            let uploaded = client.upload_stream(&mut file, size, file_name).await.map_err(|error| error.to_string())?;
            let mut input = grammers_client::InputMessage::new().text(text).file(uploaded);
            input = input.reply_to(Some(topic.topic_id));
            client.send_message(&destination_peer, input).await.map_err(|error| error.to_string())?
        } else {
            let mut input = grammers_client::InputMessage::new().text(text);
            input = input.reply_to(Some(topic.topic_id));
            client.send_message(&destination_peer, input).await.map_err(|error| error.to_string())?
        };
        if let Some(path) = temp_media { let _ = tokio::fs::remove_file(path).await; }
        created.push(sent.id());
    }
    Ok(created)
}

#[tauri::command]
pub async fn tcloud_forum_migrate_channel(
    source_channel_id: i64,
    forum_id: Option<i64>,
    delete_source_after: bool,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
    state: State<'_, crate::commands::TelegramState>,
) -> Result<ForumMigrationResult, String> {
    let client = state.client.lock().await.clone()
        .ok_or_else(|| "TCloud está sem conexão. Tente novamente quando a internet voltar.".to_string())?;

    let source_peer = state.peer_cache.read().await
        .get(&source_channel_id)
        .cloned()
        .ok_or_else(|| "Canal antigo não encontrado no cache. Sincronize as pastas e tente novamente.".to_string())?;

    let source_name = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut stmt = conn.prepare(
            "SELECT name FROM folder_metadata WHERE channel_id=?1 LIMIT 1"
        ).map_err(|e| e.to_string())?;
        stmt.bind((1, source_channel_id)).map_err(|e| e.to_string())?;
        if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
            clean_legacy_channel_name(&stmt.read::<String, _>(0).map_err(|e| e.to_string())?)
        } else {
            format!("Canal {}", source_channel_id)
        }
    };

    let topic_folder = create_topic_internal(
        &client,
        &db,
        &state.peer_cache,
        &source_name,
        forum_id,
    ).await?;
    let topic = topic_target(&db, Some(topic_folder.id))?
        .ok_or_else(|| "Tópico recém-criado não pôde ser resolvido".to_string())?;
    let destination_peer = state.peer_cache.read().await
        .get(&topic_folder.id)
        .cloned()
        .ok_or_else(|| "Fórum de destino não encontrado no cache".to_string())?;

    let temp_dir = app.path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("forum-migration");
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|error| error.to_string())?;

    let mut migrated = 0usize;
    let mut skipped = 0usize;
    let mut messages = client.iter_messages(&source_peer);

    while let Some(message) = messages.next().await.map_err(|error| error.to_string())? {
        let text = message.text().trim().to_string();
        let temp_media = download_message_media_to_temp(&client, &message, &temp_dir).await?;

        let send_result = if let Some(path) = temp_media.as_ref() {
            let mut file = tokio::fs::File::open(path).await.map_err(|error| error.to_string())?;
            let size = file.metadata().await.map_err(|error| error.to_string())?.len() as usize;
            let file_name = path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("arquivo.bin")
                .to_string();

            let uploaded = client.upload_stream(&mut file, size, file_name)
                .await
                .map_err(|error| error.to_string())?;

            let mut input = grammers_client::InputMessage::new()
                .text(text.clone())
                .file(uploaded);
            input = input.reply_to(Some(topic.topic_id));
            client.send_message(&destination_peer, input).await
        } else if !text.is_empty() {
            let mut input = grammers_client::InputMessage::new().text(text);
            input = input.reply_to(Some(topic.topic_id));
            client.send_message(&destination_peer, input).await
        } else {
            skipped += 1;
            continue;
        };

        if let Some(path) = temp_media {
            let _ = tokio::fs::remove_file(path).await;
        }

        match send_result {
            Ok(_) => migrated += 1,
            Err(error) => {
                return Err(format!(
                    "Migração interrompida após {} mensagens: {}",
                    migrated, error
                ));
            }
        }
    }

    let mut source_deleted = false;
    if delete_source_after {
        let input_channel = match source_peer {
            Peer::Channel(channel) => tl::enums::InputChannel::Channel(
                tl::types::InputChannel {
                    channel_id: channel.raw.id,
                    access_hash: channel.raw.access_hash
                        .ok_or_else(|| "Canal antigo sem access_hash".to_string())?,
                }
            ),
            _ => return Err("A origem da migração não é um canal/supergrupo".to_string()),
        };

        client.invoke(&tl::functions::channels::DeleteChannel {
            channel: input_channel,
        }).await.map_err(|error| format!(
            "Os arquivos foram migrados, mas o canal antigo não pôde ser excluído: {}",
            error
        ))?;
        source_deleted = true;
        state.peer_cache.write().await.remove(&source_channel_id);

        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut stmt = conn.prepare(
            "DELETE FROM folder_metadata WHERE channel_id=?1"
        ).map_err(|e| e.to_string())?;
        stmt.bind((1, source_channel_id)).map_err(|e| e.to_string())?;
        stmt.next().map_err(|e| e.to_string())?;
    }

    if let Err(error)=crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await{
        log::warn!("TCloud Explorer: migração concluída, mas atualização das pastas foi adiada: {}",error);
    }
    Ok(ForumMigrationResult {
        source_channel_id,
        source_name,
        topic_folder_id: topic_folder.id,
        migrated_messages: migrated,
        skipped_messages: skipped,
        source_deleted,
    })
}

pub async fn rename_topic_internal(
    client:&grammers_client::Client,
    db:&DbConnection,
    folder_id:i64,
    new_name:&str,
)->Result<(),String>{
    let clean = new_name.trim();
    if clean.is_empty() { return Err("Nome da pasta vazio".to_string()); }
    let t=topic_target(db,Some(folder_id))?.ok_or_else(||"Pasta não encontrada".to_string())?;
    if topic_name_conflicts(db, t.forum_id, t.local_id, clean)? {
        return Err("Já existe uma pasta com esse nome neste local.".to_string());
    }
    client.invoke(&tl::functions::messages::EditForumTopic{
        peer:input_peer(t.chat_id,t.access_hash),topic_id:t.topic_id,title:Some(clean.into()),icon_emoji_id:None,closed:None,hidden:None
    }).await.map_err(|e|e.to_string())?;
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let mut q=conn.prepare("UPDATE tcloud_forum_topics SET name=?1,updated_at=?2 WHERE id=?3").map_err(|e|e.to_string())?;
    q.bind((1,clean)).map_err(|e|e.to_string())?;
    q.bind((2,chrono::Utc::now().timestamp())).map_err(|e|e.to_string())?;
    q.bind((3,t.local_id)).map_err(|e|e.to_string())?;
    q.next().map_err(|e|e.to_string())?;
    let mut m=conn.prepare("UPDATE folder_metadata SET name=?1 WHERE channel_id=?2").map_err(|e|e.to_string())?;
    m.bind((1,clean)).map_err(|e|e.to_string())?;
    m.bind((2,folder_id)).map_err(|e|e.to_string())?;
    m.next().map_err(|e|e.to_string())?;
    Ok(())
}

pub async fn delete_topic_internal(
    client:&grammers_client::Client,
    db:&DbConnection,
    folder_id:i64,
)->Result<(),String>{
    let t=topic_target(db,Some(folder_id))?.ok_or_else(||"Pasta não encontrada".to_string())?;
    client.invoke(&tl::functions::messages::DeleteTopicHistory{
        peer:input_peer(t.chat_id,t.access_hash),
        top_msg_id:t.topic_id,
    }).await.map_err(|e|e.to_string())?;
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    // Files already synchronized to this topic must disappear from Explorer as
    // part of the same folder deletion. Never overwrite pending local work.
    if let Ok(mut files)=conn.prepare(
        "UPDATE tcloud_files SET sync_status='remote_deleted',updated_at=?1          WHERE COALESCE(telegram_chat_id,0)=?2            AND COALESCE(sync_status,'') NOT IN ('pending_upload','uploading','pending_move','conflict')"
    ){
        let _=files.bind((1,now));
        let _=files.bind((2,folder_id));
        let _=files.next();
    }
    let mut q=conn.prepare("DELETE FROM tcloud_forum_topics WHERE id=?1").map_err(|e|e.to_string())?;
    q.bind((1,t.local_id)).map_err(|e|e.to_string())?;
    q.next().map_err(|e|e.to_string())?;
    let mut m=conn.prepare("DELETE FROM folder_metadata WHERE channel_id=?1").map_err(|e|e.to_string())?;
    m.bind((1,folder_id)).map_err(|e|e.to_string())?;
    m.next().map_err(|e|e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn tcloud_forum_transfer_topic(
    folder_id: i64,
    target_forum_id: i64,
    move_topic: bool,
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
    state: State<'_, crate::commands::TelegramState>,
) -> Result<FolderMetadata, String> {
    if folder_id >= 0 {
        return Err("A pasta selecionada não pode ser movida desta forma".to_string());
    }
    let source = topic_target(&db, Some(folder_id))?
        .ok_or_else(|| "Pasta de origem não encontrada".to_string())?;
    if move_topic && source.forum_id == target_forum_id {
        return Err("A pasta já está neste local".to_string());
    }

    let client = state.client.lock().await.clone()
        .ok_or_else(|| "Telegram não está conectado".to_string())?;
    let source_peer = match state.peer_cache.read().await.get(&source.chat_id).cloned() {
        Some(peer) => peer,
        None => resolve_forum_peer(&client, source.chat_id, &state.peer_cache).await?,
    };

    // Match the Windows Explorer expectation when the destination already has
    // a folder with the same name: keep both by choosing "Nome (2)", "Nome (3)", ...
    // instead of creating ambiguous duplicate topic titles.
    let destination_name = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut query = conn.prepare(
            "SELECT name FROM tcloud_forum_topics WHERE forum_id=?1 ORDER BY name COLLATE NOCASE"
        ).map_err(|e| e.to_string())?;
        query.bind((1, target_forum_id)).map_err(|e| e.to_string())?;
        let mut used = HashSet::<String>::new();
        while let sqlite::State::Row = query.next().map_err(|e| e.to_string())? {
            let name = query.read::<String, _>(0).map_err(|e| e.to_string())?;
            used.insert(name.trim().to_lowercase());
        }
        let base = source.name.trim().to_string();
        if !used.contains(&base.to_lowercase()) {
            base
        } else {
            let mut suffix = 2usize;
            loop {
                let candidate = format!("{} ({})", base, suffix);
                if !used.contains(&candidate.to_lowercase()) { break candidate; }
                suffix += 1;
            }
        }
    };

    let destination = create_topic_internal(
        &client,
        &db,
        &state.peer_cache,
        &destination_name,
        Some(target_forum_id),
    ).await?;

    let mut message_ids = Vec::<i32>::new();
    let mut iter = client.iter_messages(&source_peer);
    while let Some(message) = iter.next().await.map_err(|e| e.to_string())? {
        let belongs = match &message.raw {
            tl::enums::Message::Message(raw) => match &raw.reply_to {
                Some(tl::enums::MessageReplyHeader::Header(header)) => {
                    header.reply_to_top_id == Some(source.topic_id)
                        || (header.reply_to_top_id.is_none()
                            && header.reply_to_msg_id == Some(source.topic_id))
                }
                _ => false,
            },
            _ => false,
        };
        if belongs { message_ids.push(message.id()); }
    }

    if !message_ids.is_empty() {
        let created = copy_messages_to_topic_internal(
            &client,
            &db,
            &state.peer_cache,
            &source_peer,
            &message_ids,
            destination.id,
            &app,
        ).await?;
        if created.len() != message_ids.len() {
            return Err(format!(
                "Transferência incompleta: {} de {} mensagens copiadas",
                created.len(), message_ids.len()
            ));
        }
    }

    if move_topic {
        delete_topic_internal(&client, &db, folder_id).await?;
    }
    if let Err(error)=crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app).await{
        log::warn!("TCloud Explorer: transferência de pasta concluída, mas atualização local foi adiada: {}",error);
    }
    Ok(destination)
}


// -----------------------------------------------------------------------------
// Explorer 1:1 namespace bridge (12.8 / Explorer Logic Stage 4)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ExplorerFolderOp {
    Create { relative_path: String },
    Rename { old_relative_path: String, new_relative_path: String },
    Remove { relative_path: String },
}

fn ensure_explorer_folder_ops_schema(conn: &sqlite::Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tcloud_explorer_folder_ops (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            op_type TEXT NOT NULL,
            old_relative_path TEXT,
            new_relative_path TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            next_attempt_at INTEGER,
            last_error TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            completed_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_explorer_folder_ops_ready
            ON tcloud_explorer_folder_ops(status,next_attempt_at,created_at,id);"
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn enqueue_explorer_folder_op(db: &DbConnection, op: ExplorerFolderOp) -> Result<(), String> {
    let (op_type, old_path, new_path) = match op {
        ExplorerFolderOp::Create { relative_path } => ("create", None, Some(relative_path)),
        ExplorerFolderOp::Rename { old_relative_path, new_relative_path } => ("rename", Some(old_relative_path), Some(new_relative_path)),
        ExplorerFolderOp::Remove { relative_path } => ("remove", Some(relative_path), None),
    };
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_explorer_folder_ops_schema(&conn)?;

    // Windows can emit duplicate/coalesced notifications for the same action.
    // Keep one durable operation so a reconnect/restart cannot multiply forums.
    let mut duplicate = conn.prepare(
        "SELECT id FROM tcloud_explorer_folder_ops
         WHERE op_type=?1 AND COALESCE(old_relative_path,'')=COALESCE(?2,'')
           AND COALESCE(new_relative_path,'')=COALESCE(?3,'')
           AND status IN ('pending','running') LIMIT 1"
    ).map_err(|e| e.to_string())?;
    duplicate.bind((1, op_type)).map_err(|e| e.to_string())?;
    duplicate.bind((2, old_path.as_deref())).map_err(|e| e.to_string())?;
    duplicate.bind((3, new_path.as_deref())).map_err(|e| e.to_string())?;
    if matches!(duplicate.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
        return Ok(());
    }
    drop(duplicate);

    let now = chrono::Utc::now().timestamp();
    let mut insert = conn.prepare(
        "INSERT INTO tcloud_explorer_folder_ops(op_type,old_relative_path,new_relative_path,status,attempts,next_attempt_at,last_error,created_at,updated_at)
         VALUES(?1,?2,?3,'pending',0,NULL,NULL,?4,?4)"
    ).map_err(|e| e.to_string())?;
    insert.bind((1, op_type)).map_err(|e| e.to_string())?;
    insert.bind((2, old_path.as_deref())).map_err(|e| e.to_string())?;
    insert.bind((3, new_path.as_deref())).map_err(|e| e.to_string())?;
    insert.bind((4, now)).map_err(|e| e.to_string())?;
    insert.next().map_err(|e| e.to_string())?;
    Ok(())
}

fn normalize_relative_folder_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn folder_parts(path: &str) -> Vec<String> {
    normalize_relative_folder_path(path)
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .map(|part| part.trim().to_string())
        .collect()
}

fn managed_source_for_relative(db: &DbConnection, relative_path: &str) -> Result<Option<(String,String)>, String> {
    let normalized = normalize_relative_folder_path(relative_path);
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut query = conn.prepare(
        "SELECT source_key,kind FROM tcloud_cfapi_managed_dirs WHERE lower(relative_path)=lower(?1) LIMIT 1"
    ).map_err(|e| e.to_string())?;
    query.bind((1, normalized.as_str())).map_err(|e| e.to_string())?;
    if matches!(query.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
        return Ok(Some((
            query.read::<String,_>(0).unwrap_or_default(),
            query.read::<String,_>(1).unwrap_or_default(),
        )));
    }
    Ok(None)
}

fn id_from_source_key(source_key: &str, prefix: &str) -> Option<i64> {
    source_key.strip_prefix(prefix)?.parse::<i64>().ok()
}

fn forum_id_for_explorer_parent(db: &DbConnection, parent_relative: &str) -> Result<Option<i64>, String> {
    if let Some((source_key, kind)) = managed_source_for_relative(db, parent_relative)? {
        if kind == "forum" {
            return Ok(id_from_source_key(&source_key, "forum:"));
        }
    }
    let parts = folder_parts(parent_relative);
    let Some(name) = parts.first() else { return Ok(None); };
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut query = conn.prepare(
        "SELECT id FROM tcloud_forums WHERE lower(name)=lower(?1) ORDER BY id LIMIT 1"
    ).map_err(|e| e.to_string())?;
    query.bind((1, name.as_str())).map_err(|e| e.to_string())?;
    if matches!(query.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
        return Ok(Some(query.read::<i64,_>(0).map_err(|e| e.to_string())?));
    }
    Ok(None)
}

async fn rename_forum_internal(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
    forum_id: i64,
    new_name: &str,
) -> Result<(), String> {
    let clean = new_name.trim();
    if clean.is_empty() { return Err("Nome da pasta vazio".to_string()); }
    if main_folder_name_conflicts(db, forum_id, clean)? {
        return Err("Já existe uma pasta principal com esse nome.".to_string());
    }
    let (chat_id, access_hash, _) = forum_row(db, forum_id)?;
    client.invoke(&tl::functions::channels::EditTitle {
        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel { channel_id: chat_id, access_hash }),
        title: clean.into(),
    }).await.map_err(|e| e.to_string())?;
    {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut update = conn.prepare("UPDATE tcloud_forums SET name=?1,updated_at=?2 WHERE id=?3")
            .map_err(|e| e.to_string())?;
        update.bind((1, clean)).map_err(|e| e.to_string())?;
        update.bind((2, chrono::Utc::now().timestamp())).map_err(|e| e.to_string())?;
        update.bind((3, forum_id)).map_err(|e| e.to_string())?;
        update.next().map_err(|e| e.to_string())?;
    }
    // Keep a usable peer in the cache; only the title changed.
    let _ = resolve_forum_peer(client, chat_id, cache).await;
    Ok(())
}

async fn delete_empty_forum_internal(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
    forum_id: i64,
) -> Result<(), String> {
    let (chat_id, access_hash, _) = forum_row(db, forum_id)?;
    {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut topics = conn.prepare("SELECT COUNT(*) FROM tcloud_forum_topics WHERE forum_id=?1")
            .map_err(|e| e.to_string())?;
        topics.bind((1, forum_id)).map_err(|e| e.to_string())?;
        let topic_count = if matches!(topics.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            topics.read::<i64,_>(0).unwrap_or(0)
        } else { 0 };
        if topic_count > 0 {
            return Err("TCLOUD_FOLDER_NOT_EMPTY: esta pasta principal ainda contém subpastas".to_string());
        }
        let mut files = conn.prepare(
            "SELECT COUNT(*) FROM tcloud_files WHERE COALESCE(telegram_chat_id,0)=?1
             AND COALESCE(sync_status,'') NOT IN ('trashed','deleted','remote_deleted','pending_delete')"
        ).map_err(|e| e.to_string())?;
        files.bind((1, chat_id)).map_err(|e| e.to_string())?;
        let file_count = if matches!(files.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            files.read::<i64,_>(0).unwrap_or(0)
        } else { 0 };
        if file_count > 0 {
            return Err("TCLOUD_FOLDER_NOT_EMPTY: esta pasta principal ainda contém arquivos".to_string());
        }
    }

    client.invoke(&tl::functions::channels::DeleteChannel {
        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel { channel_id: chat_id, access_hash }),
    }).await.map_err(|e| e.to_string())?;
    remove_stale_forum_local(db, forum_id)?;
    cache.write().await.remove(&chat_id);
    Ok(())
}

async fn transfer_topic_from_explorer(
    client: &grammers_client::Client,
    db: &DbConnection,
    cache: &Arc<tokio::sync::RwLock<HashMap<i64, Peer>>>,
    folder_id: i64,
    target_forum_id: i64,
    desired_name: &str,
    app: &AppHandle,
) -> Result<(), String> {
    let source = topic_target(db, Some(folder_id))?
        .ok_or_else(|| "Pasta de origem não encontrada".to_string())?;
    if source.forum_id == target_forum_id {
        if !source.name.eq_ignore_ascii_case(desired_name) {
            rename_topic_internal(client, db, folder_id, desired_name).await?;
        }
        return Ok(());
    }
    let source_peer = match cache.read().await.get(&source.chat_id).cloned() {
        Some(peer) => peer,
        None => resolve_forum_peer(client, source.chat_id, cache).await?,
    };

    let destination = create_topic_internal(client, db, cache, desired_name, Some(target_forum_id)).await?;
    let mut message_ids = Vec::<i32>::new();
    let mut iter = client.iter_messages(&source_peer);
    while let Some(message) = iter.next().await.map_err(|e| e.to_string())? {
        let belongs = match &message.raw {
            tl::enums::Message::Message(raw) => match &raw.reply_to {
                Some(tl::enums::MessageReplyHeader::Header(header)) => {
                    header.reply_to_top_id == Some(source.topic_id)
                        || (header.reply_to_top_id.is_none() && header.reply_to_msg_id == Some(source.topic_id))
                }
                _ => false,
            },
            _ => false,
        };
        if belongs { message_ids.push(message.id()); }
    }
    if !message_ids.is_empty() {
        let created = copy_messages_to_topic_internal(
            client, db, cache, &source_peer, &message_ids, destination.id, app,
        ).await?;
        if created.len() != message_ids.len() {
            return Err(format!("Movimentação incompleta: {} de {} arquivos copiados", created.len(), message_ids.len()));
        }
    }
    delete_topic_internal(client, db, folder_id).await?;
    Ok(())
}

async fn apply_explorer_folder_op(app: &AppHandle, op: ExplorerFolderOp) -> Result<(), String> {
    let db = app.try_state::<DbConnection>()
        .ok_or_else(|| "Banco do TCloud indisponível".to_string())?
        .inner().clone();
    let telegram = app.try_state::<crate::commands::TelegramState>()
        .ok_or_else(|| "Estado do Telegram indisponível".to_string())?;
    let client = telegram.client.lock().await.clone()
        .ok_or_else(|| "Telegram não conectado".to_string())?;

    match op {
        ExplorerFolderOp::Create { relative_path } => {
            let path = normalize_relative_folder_path(&relative_path);
            let parts = folder_parts(&path);
            match parts.len() {
                0 => {}
                1 => {
                    // If reconciliation already recognizes this directory, the
                    // filesystem event came from TCloud itself and needs no echo.
                    if managed_source_for_relative(&db, &path)?.is_none() {
                        let exists = {
                            let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
                            let mut q = conn.prepare("SELECT id FROM tcloud_forums WHERE lower(name)=lower(?1) LIMIT 1")
                                .map_err(|e| e.to_string())?;
                            q.bind((1, parts[0].as_str())).map_err(|e| e.to_string())?;
                            matches!(q.next().map_err(|e| e.to_string())?, sqlite::State::Row)
                        };
                        if !exists { create_forum(&client, &db, &telegram.peer_cache, &parts[0], false).await?; }
                    }
                }
                2 => {
                    if managed_source_for_relative(&db, &path)?.is_none() {
                        let forum_id = forum_id_for_explorer_parent(&db, &parts[0])?
                            .ok_or_else(|| format!("Pasta principal '{}' ainda não está pronta", parts[0]))?;
                        let exists = {
                            let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
                            let mut q = conn.prepare("SELECT id FROM tcloud_forum_topics WHERE forum_id=?1 AND lower(name)=lower(?2) LIMIT 1")
                                .map_err(|e| e.to_string())?;
                            q.bind((1, forum_id)).map_err(|e| e.to_string())?;
                            q.bind((2, parts[1].as_str())).map_err(|e| e.to_string())?;
                            matches!(q.next().map_err(|e| e.to_string())?, sqlite::State::Row)
                        };
                        if !exists { create_topic_internal(&client, &db, &telegram.peer_cache, &parts[1], Some(forum_id)).await?; }
                    }
                }
                _ => {
                    // Deeper directories are local organization inside a TCloud
                    // destination. Their files inherit the first forum/topic pair.
                }
            }
        }
        ExplorerFolderOp::Rename { old_relative_path, new_relative_path } => {
            let old_path = normalize_relative_folder_path(&old_relative_path);
            let new_path = normalize_relative_folder_path(&new_relative_path);
            let source = managed_source_for_relative(&db, &old_path)?;
            if let Some((source_key, kind)) = source {
                if kind == "forum" {
                    let forum_id = id_from_source_key(&source_key, "forum:")
                        .ok_or_else(|| "Identidade da pasta principal inválida".to_string())?;
                    let new_parts = folder_parts(&new_path);
                    if new_parts.len() != 1 { return Err("Uma pasta principal não pode ser movida para dentro de outra pasta".to_string()); }
                    rename_forum_internal(&client, &db, &telegram.peer_cache, forum_id, &new_parts[0]).await?;
                    crate::commands::tcloud_cfapi::rebind_managed_directory_prefix(&db, &old_path, &new_path)?;
                } else if kind == "topic" {
                    let local_id = id_from_source_key(&source_key, "topic:")
                        .ok_or_else(|| "Identidade da subpasta inválida".to_string())?;
                    let new_parts = folder_parts(&new_path);
                    if new_parts.len() < 2 { return Err("A subpasta precisa permanecer dentro de uma pasta principal".to_string()); }
                    let target_parent = new_parts[..new_parts.len()-1].join("/");
                    // Telegram has one forum/topic layer. Moving a topic between
                    // first-level locations is supported; deeper local folders do
                    // not create extra Telegram entities.
                    if new_parts.len() == 2 {
                        let target_forum_id = forum_id_for_explorer_parent(&db, &target_parent)?
                            .ok_or_else(|| "Pasta principal de destino ainda não está pronta".to_string())?;
                        let source_folder_id = -local_id.abs();
                        let source_topic = topic_target(&db, Some(source_folder_id))?
                            .ok_or_else(|| "Pasta de origem não encontrada".to_string())?;
                        if source_topic.forum_id == target_forum_id {
                            if !source_topic.name.eq_ignore_ascii_case(&new_parts[1]) {
                                rename_topic_internal(&client, &db, source_folder_id, &new_parts[1]).await?;
                            }
                            crate::commands::tcloud_cfapi::rebind_managed_directory_prefix(&db, &old_path, &new_path)?;
                        } else {
                            transfer_topic_from_explorer(
                                &client, &db, &telegram.peer_cache, source_folder_id, target_forum_id, &new_parts[1], app,
                            ).await?;
                        }
                    }
                }
            }
        }
        ExplorerFolderOp::Remove { relative_path } => {
            let path = normalize_relative_folder_path(&relative_path);
            if let Some((source_key, kind)) = managed_source_for_relative(&db, &path)? {
                if kind == "topic" {
                    let local_id = id_from_source_key(&source_key, "topic:")
                        .ok_or_else(|| "Identidade da subpasta inválida".to_string())?;
                    delete_topic_internal(&client, &db, -local_id.abs()).await?;
                } else if kind == "forum" {
                    let forum_id = id_from_source_key(&source_key, "forum:")
                        .ok_or_else(|| "Identidade da pasta principal inválida".to_string())?;
                    delete_empty_forum_internal(&client, &db, &telegram.peer_cache, forum_id).await?;
                }
            }
        }
    }

    crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await?;
    let _ = app.emit("tcloud-folder-tree-changed", serde_json::json!({"source":"explorer"}));
    Ok(())
}

pub fn start_explorer_namespace_worker(app: AppHandle) {
    use std::sync::OnceLock;
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() { return; }

    let db = match app.try_state::<DbConnection>() {
        Some(db) => db.inner().clone(),
        None => {
            log::warn!("TCloud Explorer: banco indisponível para iniciar worker de pastas");
            return;
        }
    };
    if let Ok(conn) = db.lock() {
        if ensure_explorer_folder_ops_schema(&conn).is_ok() {
            let _ = conn.execute("UPDATE tcloud_explorer_folder_ops SET status='pending',next_attempt_at=NULL WHERE status='running'");
        }
    }

    tauri::async_runtime::spawn(async move {
        loop {
            let claimed = (|| -> Option<(i64,String,Option<String>,Option<String>,i64)> {
                let conn = db.lock().ok()?;
                ensure_explorer_folder_ops_schema(&conn).ok()?;
                let now = chrono::Utc::now().timestamp();
                let mut q = conn.prepare(
                    "SELECT id,op_type,old_relative_path,new_relative_path,attempts
                     FROM tcloud_explorer_folder_ops
                     WHERE status='pending' AND (next_attempt_at IS NULL OR next_attempt_at<=?1)
                     ORDER BY created_at ASC,id ASC LIMIT 1"
                ).ok()?;
                q.bind((1, now)).ok()?;
                if !matches!(q.next().ok()?, sqlite::State::Row) { return None; }
                let row = (
                    q.read::<i64,_>(0).unwrap_or(0),
                    q.read::<String,_>(1).unwrap_or_default(),
                    q.read::<Option<String>,_>(2).unwrap_or(None),
                    q.read::<Option<String>,_>(3).unwrap_or(None),
                    q.read::<i64,_>(4).unwrap_or(0),
                );
                drop(q);
                if let Ok(mut mark) = conn.prepare("UPDATE tcloud_explorer_folder_ops SET status='running',updated_at=?1 WHERE id=?2") {
                    let _ = mark.bind((1, now));
                    let _ = mark.bind((2, row.0));
                    let _ = mark.next();
                }
                Some(row)
            })();

            let Some((id, op_type, old_path, new_path, attempts)) = claimed else {
                tokio::time::sleep(std::time::Duration::from_millis(650)).await;
                continue;
            };
            let op = match op_type.as_str() {
                "create" => new_path.clone().map(|relative_path| ExplorerFolderOp::Create { relative_path }),
                "rename" => match (old_path.clone(), new_path.clone()) {
                    (Some(old_relative_path), Some(new_relative_path)) => Some(ExplorerFolderOp::Rename { old_relative_path, new_relative_path }),
                    _ => None,
                },
                "remove" => old_path.clone().map(|relative_path| ExplorerFolderOp::Remove { relative_path }),
                _ => None,
            };

            let result = match op {
                Some(op) => apply_explorer_folder_op(&app, op).await,
                None => Err("Operação de pasta inválida".to_string()),
            };
            let now = chrono::Utc::now().timestamp();
            match result {
                Ok(()) => {
                    if let Ok(conn) = db.lock() {
                        if let Ok(mut done) = conn.prepare("UPDATE tcloud_explorer_folder_ops SET status='done',last_error=NULL,completed_at=?1,updated_at=?1 WHERE id=?2") {
                            let _=done.bind((1,now)); let _=done.bind((2,id)); let _=done.next();
                        }
                        let cutoff = now - 7 * 24 * 60 * 60;
                        if let Ok(mut cleanup)=conn.prepare("DELETE FROM tcloud_explorer_folder_ops WHERE status='done' AND completed_at<?1") {
                            let _=cleanup.bind((1,cutoff)); let _=cleanup.next();
                        }
                    }
                }
                Err(error) => {
                    if error.starts_with("TCLOUD_FOLDER_NOT_EMPTY:") {
                        // Protect data: Explorer may emit only the parent delete
                        // event for a non-empty tree. Restore the namespace rather
                        // than deleting an entire Telegram forum implicitly.
                        log::warn!("TCloud Explorer: {}. A pasta será restaurada para proteger os arquivos.", error.trim_start_matches("TCLOUD_FOLDER_NOT_EMPTY: "));
                        let _ = crate::commands::tcloud_cfapi::refresh_folder_namespace_now(app.clone()).await;
                        if let Ok(conn)=db.lock() {
                            if let Ok(mut done)=conn.prepare("UPDATE tcloud_explorer_folder_ops SET status='done',last_error=?1,completed_at=?2,updated_at=?2 WHERE id=?3") {
                                let _=done.bind((1,error.as_str())); let _=done.bind((2,now)); let _=done.bind((3,id)); let _=done.next();
                            }
                        }
                        continue;
                    }
                    let next_attempt = attempts + 1;
                    let delay = match next_attempt { 0..=1 => 2, 2 => 5, 3 => 10, 4 => 30, 5 => 60, 6 => 120, _ => 300 };
                    log::warn!("TCloud Explorer: operação de pasta será repetida em {}s: {}", delay, error);
                    if let Ok(conn)=db.lock() {
                        if let Ok(mut retry)=conn.prepare(
                            "UPDATE tcloud_explorer_folder_ops SET status='pending',attempts=?1,next_attempt_at=?2,last_error=?3,updated_at=?4 WHERE id=?5"
                        ) {
                            let _=retry.bind((1,next_attempt)); let _=retry.bind((2,now+delay)); let _=retry.bind((3,error.as_str())); let _=retry.bind((4,now)); let _=retry.bind((5,id)); let _=retry.next();
                        }
                    }
                }
            }
        }
    });
}
