import { invoke } from '@tauri-apps/api/core';

export interface ProIndexStats {
  files: number;
  folders: number;
  indexed_bytes: number;
  cached_files: number;
  offline_files: number;
  pending_jobs: number;
  conflicts: number;
  trash_items: number;
  versions: number;
}

export interface ProTrashItem {
  id: number;
  message_id: number;
  folder_id: number | null;
  name: string;
  size: number;
  deleted_at: number;
  relative_path?: string | null;
  expires_at?: number | null;
}

export interface ProTrashBatchResult {
  restored: number;
  failed: number;
  errors: string[];
}

export interface ProVersionItem {
  id: number;
  message_id: number;
  folder_id: number | null;
  name: string;
  size: number;
  note?: string | null;
  created_at: number;
}

export interface ProRule {
  id: number;
  name: string;
  extensions: string;
  target_folder_id: number | null;
  enabled: boolean;
  name_contains?: string | null;
  min_size?: number | null;
  max_size?: number | null;
}

export interface ProBackupSource {
  id: number;
  local_path: string;
  target_folder_id: number | null;
  enabled: boolean;
  interval_minutes: number;
  last_run_at: number | null;
  last_scan_at?: number | null;
  last_success_at?: number | null;
}

export interface ProBackupScanResult {
  files_found: number;
  total_bytes: number;
}

export const getProIndexStats = () => invoke<ProIndexStats>('tcloud_pro_index_stats');
export const markProIndexRebuilt = () => invoke<number>('tcloud_pro_mark_index_rebuilt');
export const listTrash = () => invoke<ProTrashItem[]>('tcloud_pro_trash_list');
export const restoreTrash = (trashId: number) => invoke<boolean>('tcloud_pro_trash_restore', { trashId });
export const restoreTrashMany = (trashIds: number[]) => invoke<ProTrashBatchResult>('tcloud_pro_trash_restore_many', { trashIds });
export const clearTrash = () => invoke<boolean>('tcloud_pro_trash_clear');
export const addVersion = (messageId: number, folderId: number | null, name: string, size: number, note?: string | null) =>
  invoke<number>('tcloud_pro_version_add', { messageId, folderId, name, size, note: note ?? null });
export const listVersions = (name?: string | null) => invoke<ProVersionItem[]>('tcloud_pro_version_list', { name: name ?? null });
export const listRules = () => invoke<ProRule[]>('tcloud_pro_rules_list');
export const saveRule = (payload: Omit<ProRule, 'id'> & { id?: number }) => invoke<number>('tcloud_pro_rule_save', {
  id: payload.id ?? null,
  name: payload.name,
  extensions: payload.extensions,
  targetFolderId: payload.target_folder_id,
  enabled: payload.enabled,
  nameContains: payload.name_contains ?? null,
  minSize: payload.min_size ?? null,
  maxSize: payload.max_size ?? null,
});
export const deleteRule = (id: number) => invoke<boolean>('tcloud_pro_rule_delete', { id });
export const listBackups = () => invoke<ProBackupSource[]>('tcloud_pro_backups_list');
export const saveBackup = (payload: Omit<ProBackupSource, 'id' | 'last_run_at'> & { id?: number }) => invoke<number>('tcloud_pro_backup_save', {
  id: payload.id ?? null,
  localPath: payload.local_path,
  targetFolderId: payload.target_folder_id,
  enabled: payload.enabled,
  intervalMinutes: payload.interval_minutes,
});
export const deleteBackup = (id: number) => invoke<boolean>('tcloud_pro_backup_delete', { id });
export const scanBackup = (id: number) => invoke<ProBackupScanResult>('tcloud_pro_backup_scan', { id });
export const pinOffline = (fileId: number, pinned: boolean) => invoke<boolean>('tcloud_pro_offline_pin', { fileId, pinned });

export const copyTelegramFiles = (messageIds: number[], sourceFolderId: number | null, targetFolderId: number | null) =>
  invoke<number[]>('cmd_copy_files', { messageIds, sourceFolderId, targetFolderId });

export const formatBytesCompact = (bytes: number) => {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value >= 10 || index === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
};

export const listBackupFiles = (id: number) => invoke<string[]>('tcloud_pro_backup_files', { id });
export const markBackupRun = (id: number) => invoke<number>('tcloud_pro_backup_mark_run', { id });
export const markBackupFile = (id: number, filePath: string) => invoke<boolean>('tcloud_pro_backup_mark_file', { id, filePath });

export interface ProIndexFileRow { message_id: number; name: string; size: number; created_at?: string | null; sync_status: string; local_status: string; is_pinned: boolean; }
export const listIndexedFiles = (folderId: number | null) => invoke<ProIndexFileRow[]>('tcloud_pro_index_list_files', { folderId });
export const upsertIndexedFiles = (folderId: number | null, files: Array<{message_id:number;name:string;size:number;mime_type?:string|null;created_at?:string|null}>) => invoke<number>('tcloud_pro_index_upsert_files', { folderId, files });


export interface ProDuplicateFile { file_id:number; telegram_message_id:number|null; telegram_chat_id:number|null; name:string; size:number; sha256:string; }
export interface ProDuplicateGroup { sha256:string; size:number; reclaimable_bytes:number; files:ProDuplicateFile[]; }
export interface ProCleanupStats { cached_files:number; cached_bytes:number; duplicate_groups:number; duplicate_reclaimable_bytes:number; trash_items:number; versions:number; }
export interface ProContentHit { file_id:number; telegram_message_id:number|null; telegram_chat_id:number|null; name:string; size:number; snippet:string; }
export interface ProActivityItem { id:number; kind:string; title:string; detail?:string|null; created_at:number; }

export const listDuplicates = () => invoke<ProDuplicateGroup[]>('tcloud_pro_duplicates');
export const getCleanupStats = () => invoke<ProCleanupStats>('tcloud_pro_cleanup_stats');
export const reindexTextContent = () => invoke<number>('tcloud_pro_reindex_text_content');
export const searchIndexedContent = (query:string) => invoke<ProContentHit[]>('tcloud_pro_content_search', { query });
export const addActivity = (kind:string,title:string,detail?:string|null) => invoke<number>('tcloud_pro_activity_add',{kind,title,detail:detail??null});
export const listActivity = (limit=100) => invoke<ProActivityItem[]>('tcloud_pro_activity_list',{limit});
export const pauseAllJobs = (paused:boolean) => invoke<number>('tcloud_pro_pause_all_jobs',{paused});
export const sendDesktopNotification = (title:string, body:string) => invoke<boolean>('tcloud_pro_notify',{title,body});

export interface ProFileMetadata {
  message_id: number;
  folder_id: number | null;
  tags: string[];
  note: string;
  updated_at: number;
}
export interface ProStorageBucket { category: string; files: number; bytes: number; }
export interface ProDuplicateSource { name:string; message_id:number; folder_id:number|null; }
export interface ProDuplicateMatch { sha256: string; size: number; matches: number; names: string[]; sources:ProDuplicateSource[]; }

export const getFileMetadata = (messageId:number, folderId:number|null) =>
  invoke<ProFileMetadata>('tcloud_pro_metadata_get', { messageId, folderId });
export const setFileMetadata = (messageId:number, folderId:number|null, tags:string[], note:string) =>
  invoke<ProFileMetadata>('tcloud_pro_metadata_set', { messageId, folderId, tags, note });
export const getStorageBreakdown = () => invoke<ProStorageBucket[]>('tcloud_pro_storage_breakdown');
export const checkLocalDuplicate = (path:string) => invoke<ProDuplicateMatch>('tcloud_pro_local_duplicate_check', { path });
export const exportDiagnostics = (outputPath:string) => invoke<string>('tcloud_pro_export_diagnostics', { outputPath });
export const exportConfigJson = (outputPath:string, json:string) => invoke<string>('tcloud_pro_export_config', { outputPath, json });
export const importConfigJson = (path:string) => invoke<string>('tcloud_pro_import_config', { path });
