import { invoke } from '@tauri-apps/api/core';

export type FolderPolicyMode = 'normal' | 'offline' | 'online';
export interface FolderSyncPolicy { relative_path:string; mode:FolderPolicyMode; updated_at:number; }
export interface SyncMaturitySettings { version_retention:number; trash_retention_days:number; conflict_strategy:'keep_both'|'ask'; }
export interface SyncConflict {
  id:number; file_id:number; local_hash?:string|null; remote_hash?:string|null; resolution:string; created_at:number;
  remote_message_id?:number|null; file_name?:string|null; local_path?:string|null; remote_name?:string|null; local_size?:number|null; remote_size?:number|null;
}

export const listFolderSyncPolicies=()=>invoke<FolderSyncPolicy[]>('tcloud_sync_maturity_list_policies');
export const setFolderSyncPolicy=(relativePath:string,mode:FolderPolicyMode)=>invoke<FolderSyncPolicy>('tcloud_sync_maturity_set_policy',{relativePath,mode});
export const getSyncMaturitySettings=()=>invoke<SyncMaturitySettings>('tcloud_sync_maturity_get_settings');
export const setSyncMaturitySettings=(settings:SyncMaturitySettings)=>invoke<SyncMaturitySettings>('tcloud_sync_maturity_set_settings',{
  versionRetention:settings.version_retention,trashRetentionDays:settings.trash_retention_days,conflictStrategy:settings.conflict_strategy,
});
export const purgeTrash=(trashId?:number|null)=>invoke<number>('tcloud_sync_maturity_purge_trash',{trashId:trashId??null});
export const listSyncConflicts=()=>invoke<SyncConflict[]>('tcloud_list_conflicts');
export const resolveSyncConflict=(conflictId:number,resolution:'keep_local'|'keep_remote'|'keep_both')=>invoke<void>('tcloud_resolve_conflict',{conflictId,resolution});
