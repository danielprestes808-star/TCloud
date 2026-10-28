import { lazy, Suspense, useState, useEffect, useCallback, useRef, useMemo, useDeferredValue, type MouseEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import {
    closestCenter,
    DndContext,
    DragEndEvent,
    DragOverlay,
    DragOverEvent,
    DragStartEvent,
    KeyboardSensor,
    PointerSensor,
    useSensor,
    useSensors,
} from '@dnd-kit/core';
import { arrayMove, sortableKeyboardCoordinates } from '@dnd-kit/sortable';

import { TelegramFile, BandwidthStats, ShareInfo } from '../../types';
import { formatBytes, isMediaFile, isPdfFile, isArchiveFile, nativeShareOrCopy, copyToClipboard } from '../../utils';

// Components
import { Sidebar } from './dashboard/Sidebar';
import { TopBar, type ExplorerBreadcrumbItem } from './dashboard/TopBar';
import { ExplorerStatusBar } from './dashboard/ExplorerStatusBar';
import { FileExplorer, type SortDirection, type SortField } from './dashboard/FileExplorer';
import { TransferCenter } from './dashboard/TransferCenter';
import { ExternalDropBlocker } from './dashboard/ExternalDropBlocker';
import { ForumOverview } from './dashboard/ForumOverview';
import { ExplorerInlineEditor } from './dashboard/ExplorerInlineEditor';
import { ExplorerUtilityWorkspace, ExplorerVaultHeader } from './dashboard/ExplorerUtilityWorkspace';
import type { TCloudAdvancedTab } from './dashboard/TCloudAdvancedCenterModal';

import { getTCloudSetupState } from '../../services/tcloudSetup';
import { Files, Link, Copy, Check, X, Loader2, Share2, FolderOpen, Image, Video, FileText, Music, ArrowRight, HardDrive, Star, CircleAlert, RefreshCw, CheckCircle2, Cloud, CloudOff, LockKeyhole } from 'lucide-react';

// Hooks
import { useTelegramConnection } from '../../hooks/useTelegramConnection';
import { useFileOperations } from '../../hooks/useFileOperations';
import { useFileUpload } from '../../hooks/useFileUpload';
import { useFileDownload } from '../../hooks/useFileDownload';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useSettings, type ExplorerViewMode } from '../../context/SettingsContext';
import { useConfirm } from '../../context/ConfirmContext';
import { favoriteKey, getFavoriteIds, subscribeFavorites, syncFavoritesFromTelegram, type FavoriteKey } from '../../services/tcloudFavorites';
import { getCfapiStatus, openCfapiRoot, parseNaturalTCloudSearch, retryFailedCfapiOperations, setCfapiItemAvailability } from '../../services/tcloudCompletion';
import { createTCloudForum, createTCloudForumTopic, listTCloudForumTree, refreshTCloudForumTreeFromTelegram, type TCloudForumTree } from '../../services/tcloudForums';
import { listBackupFiles, listIndexedFiles, markBackupFile, markBackupRun, listTrash, searchIndexedContent, upsertIndexedFiles, type ProBackupSource } from '../../services/tcloudProductivity';
import { purgeTrash } from '../../services/tcloudSyncMaturity';
import { cleanupStaleTCloudAttention, getTCloudSyncOverview, retryAllTCloudSyncFailures } from '../../services/tcloudSyncCenter';
import { isTCloudUiBackgroundReady } from '../../services/tcloudWindowResume';
import { getFolderNavigationSnapshot, setFolderNavigationSnapshot, warmFolderNavigationSnapshot } from '../../services/tcloudFolderNavigationCache';

// Heavy/rarely-used surfaces are loaded only when the user opens them. Keeping
// them out of the initial dashboard bundle is especially important for PDF,
// adaptive MP4/HLS playback and the large settings/advanced centers.
const MoveToFolderModal = lazy(() => import('./dashboard/MoveToFolderModal').then((m) => ({ default: m.MoveToFolderModal })));
const PreviewModal = lazy(() => import('./dashboard/PreviewModal').then((m) => ({ default: m.PreviewModal })));
const MediaPlayer = lazy(() => import('./dashboard/MediaPlayer').then((m) => ({ default: m.MediaPlayer })));
const PdfViewer = lazy(() => import('./dashboard/PdfViewer').then((m) => ({ default: m.PdfViewer })));
const ArchiveViewerModal = lazy(() => import('./dashboard/ArchiveViewerModal').then((m) => ({ default: m.ArchiveViewerModal })));
const SettingsModal = lazy(() => import('./dashboard/SettingsModal').then((m) => ({ default: m.SettingsModal })));
const ShareDialog = lazy(() => import('./dashboard/ShareDialog').then((m) => ({ default: m.ShareDialog })));
const RemoteUploadModal = lazy(() => import('./dashboard/RemoteUploadModal').then((m) => ({ default: m.RemoteUploadModal })));
const TCloudDashboardModal = lazy(() => import('./dashboard/TCloudDashboardModal').then((m) => ({ default: m.TCloudDashboardModal })));
const TCloudSyncCenterModal = lazy(() => import('./dashboard/TCloudSyncCenterModal').then((m) => ({ default: m.TCloudSyncCenterModal })));
const TCloudOperationsCenterModal = lazy(() => import('./dashboard/TCloudOperationsCenterModal').then((m) => ({ default: m.TCloudOperationsCenterModal })));
const TCloudForumManagerModal = lazy(() => import('./dashboard/TCloudForumManagerModal').then((m) => ({ default: m.TCloudForumManagerModal })));
const TCloudAdvancedCenterModal = lazy(() => import('./dashboard/TCloudAdvancedCenterModal').then((m) => ({ default: m.TCloudAdvancedCenterModal })));
const FileDetailsPanel = lazy(() => import('./dashboard/FileDetailsPanel').then((m) => ({ default: m.FileDetailsPanel })));
const TCloudSetupWizard = lazy(() => import('../../pages/TCloudSetupWizard'));

type TCloudFileFilter = 'all' | 'images' | 'videos' | 'audio' | 'documents' | 'archives';
type ExplorerRootView = 'dashboard' | 'saved' | 'recent' | 'favorites' | 'problems' | 'sync' | 'trash' | 'gallery' | 'music' | 'offline' | 'vault';
type ExplorerNavigationSnapshot = { folderId: number | null; rootView: ExplorerRootView; forumId: number | null };
type TCloudDateFilter = 'any' | 'today' | '7days' | '30days';
type ExplorerClipboardMode = 'copy' | 'cut';
type ExplorerClipboardItem = { id: number; name: string; sourceFolderId: number | null };
type InlineExplorerEditor = { mode: 'create'; value: string } | { mode: 'rename'; value: string; target: TelegramFile };

function windowsLikeFolderName(baseName: string, existingNames: string[]) {
    const used = new Set(existingNames.map((name) => name.trim().toLocaleLowerCase('pt-BR')));
    if (!used.has(baseName.toLocaleLowerCase('pt-BR'))) return baseName;
    let suffix = 2;
    while (used.has(`${baseName} (${suffix})`.toLocaleLowerCase('pt-BR'))) suffix += 1;
    return `${baseName} (${suffix})`;
}

function LazyOverlayFallback() {
    return (
        <div className="pointer-events-none fixed inset-0 z-[250] flex items-center justify-center bg-black/10" aria-live="polite">
            <div className="flex items-center gap-2 rounded-lg border border-app-border bg-app-surface/95 px-3 py-2 text-sm text-app-text-secondary shadow-lg">
                <Loader2 className="h-4 w-4 animate-spin" />
                Abrindo...
            </div>
        </div>
    );
}

function sameFileSnapshot(a: TelegramFile[] | undefined, b: TelegramFile[]): boolean {
    if (!a || a.length !== b.length) return false;
    for (let i = 0; i < b.length; i += 1) {
        const left = a[i];
        const right = b[i];
        if (left.id !== right.id
            || left.name !== right.name
            || left.size !== right.size
            || left.created_at !== right.created_at
            || left.type !== right.type
            || left.folder_id !== right.folder_id
            || left.sync_status !== right.sync_status
            || left.local_status !== right.local_status
            || left.is_pinned !== right.is_pinned
            || left.encryption_state !== right.encryption_state) return false;
    }
    return true;
}

function sameForumTree(a: TCloudForumTree[] | undefined, b: TCloudForumTree[]): boolean {
    if (!a || a.length !== b.length) return false;
    for (let i = 0; i < b.length; i += 1) {
        const left = a[i];
        const right = b[i];
        if (left.id !== right.id || left.telegram_chat_id !== right.telegram_chat_id || left.name !== right.name || left.topics.length !== right.topics.length) return false;
        for (let j = 0; j < right.topics.length; j += 1) {
            if (left.topics[j]?.id !== right.topics[j].id || left.topics[j]?.name !== right.topics[j].name) return false;
        }
    }
    return true;
}

export function Dashboard({ onLogout }: { onLogout: () => void }) {
    const queryClient = useQueryClient();
    const { t } = useTranslation();


    const {
        store, folders, groups, activeFolderId, setActiveFolderId, isSyncing, isConnected,
        handleLogout, handleSyncFolders, handleCreateFolder, handleFolderDelete,
        handleFolderRename, handleFolderToggleVisibility, handleExportFolderInvite,
        handleCreateGroup, handleDeleteGroup, handleUpdateGroup, handleAssignFolderToGroup,
        handleReorderFolders, handleUpdateGroupOrder
    } = useTelegramConnection(onLogout);


    const { settings, updateSetting } = useSettings();
    const { confirm } = useConfirm();
    const viewMode = settings.viewMode;
    const setViewMode = (mode: ExplorerViewMode) => updateSetting('viewMode', mode);

    // One shared folder tree feeds the breadcrumb, sidebar and folder creation.
    // This avoids separate Telegram scans producing slightly different UI trees.
    const { data: forumTree = [] } = useQuery({
        queryKey: ['tcloud-forum-tree'],
        queryFn: listTCloudForumTree,
        enabled: isConnected,
        staleTime: 30_000,
        refetchInterval: 120_000,
        refetchIntervalInBackground: false,
    });

    useEffect(() => {
        const changed = () => void queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] });
        window.addEventListener('tcloud-forums-changed', changed);
        return () => window.removeEventListener('tcloud-forums-changed', changed);
    }, [queryClient]);

    useEffect(() => {
        if (!isConnected) return;
        let cancelled = false;
        let inFlight = false;
        const refreshRemoteTree = async () => {
            if (inFlight) return;
            if (document.visibilityState !== 'visible') return;
            if (!isTCloudUiBackgroundReady(700)) return;
            inFlight = true;
            try {
                const remoteTree = await refreshTCloudForumTreeFromTelegram();
                if (!cancelled) {
                    queryClient.setQueryData<TCloudForumTree[]>(['tcloud-forum-tree'], (current) =>
                        sameForumTree(current, remoteTree) ? current : remoteTree);
                }
            } catch (error) {
                console.warn('TCloud: atualização remota de pastas adiada', error);
            } finally {
                inFlight = false;
            }
        };
        // Startup priority:
        // 1. render local SQLite/cache
        // 2. refresh the folder currently visible
        // 3. only then perform the heavier Telegram forum-tree refresh
        //
        // An empty tree previously triggered this after only 300ms, competing
        // directly with cmd_get_files during cold startup.
        const firstDelay = forumTree.length === 0 ? 4_000 : 12_000;
        const first = window.setTimeout(() => void refreshRemoteTree(), firstDelay);
        let resumeTimer: number | null = null;
        const afterResume = () => {
            if (resumeTimer !== null) window.clearTimeout(resumeTimer);
            resumeTimer = window.setTimeout(() => {
                resumeTimer = null;
                void refreshRemoteTree();
            }, 1100);
        };
        window.addEventListener('tcloud-ui-resumed', afterResume);
        return () => {
            cancelled = true;
            window.clearTimeout(first);
            if (resumeTimer !== null) window.clearTimeout(resumeTimer);
            window.removeEventListener('tcloud-ui-resumed', afterResume);
        };
    }, [isConnected, queryClient, forumTree.length]);

    useEffect(() => {
        let refreshTimer: number | null = null;

        const refreshTree = () => {
            if (refreshTimer !== null) {
                window.clearTimeout(refreshTimer);
            }

            refreshTimer = window.setTimeout(() => {
                refreshTimer = null;

                void queryClient.invalidateQueries({
                    queryKey: ['tcloud-forum-tree']
                });

                if (activeFolderId !== null) {
                    void queryClient.invalidateQueries({
                        queryKey: ['files', activeFolderId]
                    });
                }
            }, 250);
        };

        const unlistenPromises = [
            listen('tcloud-folder-tree-changed', refreshTree),
            listen('tcloud-explorer-folders-changed', refreshTree),
        ];

        return () => {
            if (refreshTimer !== null) {
                window.clearTimeout(refreshTimer);
            }

            void Promise.all(unlistenPromises).then((unlisteners) => {
                unlisteners.forEach((unlisten) => unlisten());
            });
        };
    }, [activeFolderId, queryClient]);

    const [previewFile, setPreviewFile] = useState<TelegramFile | null>(null);
    const [selectedIds, setSelectedIds] = useState<number[]>([]);
    const selectedIdSet = useMemo(() => new Set(selectedIds), [selectedIds]);
    const [showMoveModal, setShowMoveModal] = useState(false);
    const [showSettings, setShowSettings] = useState(false);
    const [showTCloudDashboard, setShowTCloudDashboard] = useState(false);
    const [showTCloudSyncCenter, setShowTCloudSyncCenter] = useState(false);
    const [showTCloudOperations, setShowTCloudOperations] = useState(false);
    const [showForumManager, setShowForumManager] = useState(false);
    const [showAdvancedCenter, setShowAdvancedCenter] = useState(false);
    const [advancedInitialTab, setAdvancedInitialTab] = useState<TCloudAdvancedTab>('index');
    const [showDetailsPanel, setShowDetailsPanel] = useState(false);
    const [trashIds, setTrashIds] = useState<Set<number>>(new Set());
    const [selectedForum, setSelectedForum] = useState<TCloudForumTree | null>(null);

    useEffect(() => {
        if (!selectedForum) return;
        const refreshed = forumTree.find((forum) => forum.id === selectedForum.id);
        if (!refreshed) {
            setSelectedForum(null);
            setActiveFolderId(null);
            return;
        }
        const topicsChanged = refreshed.topics.length !== selectedForum.topics.length
            || refreshed.topics.some((topic, index) => topic.id !== selectedForum.topics[index]?.id || topic.name !== selectedForum.topics[index]?.name);
        if (refreshed.name !== selectedForum.name || topicsChanged) setSelectedForum(refreshed);
    }, [forumTree, selectedForum, setActiveFolderId]);
    const [inlineEditor, setInlineEditor] = useState<InlineExplorerEditor | null>(null);
    const [inlineEditorBusy, setInlineEditorBusy] = useState(false);
    const [explorerClipboard, setExplorerClipboard] = useState<{ mode: ExplorerClipboardMode; items: ExplorerClipboardItem[] } | null>(null);
    const [rootView, setRootView] = useState<ExplorerRootView>('dashboard');
    const navigationHistoryRef = useRef<ExplorerNavigationSnapshot[]>([{ folderId: null, rootView: 'dashboard', forumId: null }]);
    const navigationIndexRef = useRef(0);
    const [navigationIndex, setNavigationIndex] = useState(0);
    const suppressNavigationCaptureRef = useRef(false);

    const createFolderInCurrentContext = useCallback(async (name: string) => {
        const cleanName = name.trim();
        if (!cleanName) return;

        // Windows-like rule: at the TCloud root, "Nova pasta" creates a new
        // top-level storage location. Internally this is a Telegram forum, but
        // that implementation detail is deliberately hidden from the user.
        if (!selectedForum && activeFolderId === null) {
            if (rootView !== 'dashboard') {
                toast.info('Volte para Início para criar uma pasta principal.');
                return;
            }
            try {
                const created = await createTCloudForum(cleanName, false);
                queryClient.setQueryData<TCloudForumTree[]>(['tcloud-forum-tree'], (current = []) => {
                    if (current.some((forum) => forum.id === created.id)) return current;
                    return [...current, { ...created, topics: [] }]
                        .sort((a, b) => a.name.localeCompare(b.name, 'pt-BR', { sensitivity: 'base' }));
                });
                toast.success(`Pasta ${created.name} criada.`);
            } catch (error) {
                toast.error(`Não foi possível criar a pasta: ${String(error)}`);
            }
            return;
        }

        if (!selectedForum) {
            await handleCreateFolder(cleanName);
            return;
        }
        try {
            const created = await createTCloudForumTopic(cleanName, selectedForum.id);
            const topic = { id: created.id, name: created.name };
            setSelectedForum((current) => current && current.id === selectedForum.id
                ? { ...current, topics: current.topics.some((item) => item.id === topic.id) ? current.topics : [...current.topics, topic] }
                : current);
            queryClient.setQueryData<TCloudForumTree[]>(['tcloud-forum-tree'], (current = []) =>
                current.map((forum) => forum.id === selectedForum.id
                    ? { ...forum, topics: forum.topics.some((item) => item.id === topic.id) ? forum.topics : [...forum.topics, topic] }
                    : forum));
            toast.success(`Pasta ${created.name} criada em ${selectedForum.name}`);
        } catch (error) {
            toast.error(`Não foi possível criar a pasta: ${String(error)}`);
        }
    }, [selectedForum, activeFolderId, rootView, handleCreateFolder, queryClient]);
    useEffect(() => {
        const snapshot: ExplorerNavigationSnapshot = {
            folderId: activeFolderId,
            rootView,
            forumId: selectedForum?.id ?? null,
        };
        if (suppressNavigationCaptureRef.current) {
            suppressNavigationCaptureRef.current = false;
            return;
        }
        const history = navigationHistoryRef.current;
        const current = history[navigationIndexRef.current];
        if (current
            && current.folderId === snapshot.folderId
            && current.rootView === snapshot.rootView
            && current.forumId === snapshot.forumId) return;

        const next = history.slice(0, navigationIndexRef.current + 1);
        next.push(snapshot);
        if (next.length > 60) next.splice(0, next.length - 60);
        navigationHistoryRef.current = next;
        navigationIndexRef.current = next.length - 1;
        setNavigationIndex(navigationIndexRef.current);
    }, [activeFolderId, rootView, selectedForum?.id]);

    const applyNavigationSnapshot = useCallback((snapshot: ExplorerNavigationSnapshot) => {
        suppressNavigationCaptureRef.current = true;
        setRootView(snapshot.rootView);
        setActiveFolderId(snapshot.folderId);
        setSelectedForum(snapshot.forumId === null ? null : (forumTree.find((forum) => forum.id === snapshot.forumId) ?? null));
    }, [forumTree, setActiveFolderId]);

    const handleNavigationBack = useCallback(() => {
        const targetIndex = navigationIndexRef.current - 1;
        if (targetIndex < 0) return;
        const snapshot = navigationHistoryRef.current[targetIndex];
        if (!snapshot) return;
        navigationIndexRef.current = targetIndex;
        setNavigationIndex(targetIndex);
        applyNavigationSnapshot(snapshot);
    }, [applyNavigationSnapshot]);

    const handleNavigationForward = useCallback(() => {
        const targetIndex = navigationIndexRef.current + 1;
        if (targetIndex >= navigationHistoryRef.current.length) return;
        const snapshot = navigationHistoryRef.current[targetIndex];
        if (!snapshot) return;
        navigationIndexRef.current = targetIndex;
        setNavigationIndex(targetIndex);
        applyNavigationSnapshot(snapshot);
    }, [applyNavigationSnapshot]);

    useEffect(() => {
        const onNavigationShortcut = (event: KeyboardEvent) => {
            if (!event.altKey || event.ctrlKey || event.metaKey) return;
            if (event.key === 'ArrowLeft') {
                event.preventDefault();
                handleNavigationBack();
            } else if (event.key === 'ArrowRight') {
                event.preventDefault();
                handleNavigationForward();
            }
        };
        window.addEventListener('keydown', onNavigationShortcut);
        return () => window.removeEventListener('keydown', onNavigationShortcut);
    }, [handleNavigationBack, handleNavigationForward]);

    const [favoriteIds, setFavoriteIds] = useState<FavoriteKey[]>(() => getFavoriteIds());
    const [showTCloudSetup, setShowTCloudSetup] = useState(false);
    const [searchTerm, setSearchTerm] = useState("");
    const [searchResults, setSearchResults] = useState<TelegramFile[]>([]);
    const [isSearching, setIsSearching] = useState(false);
    const searchRequestSeqRef = useRef(0);
    const [fileFilter, setFileFilter] = useState<TCloudFileFilter>('all');
    const [dateFilter, setDateFilter] = useState<TCloudDateFilter>('any');
    const [cardScale, setCardScale] = useState(() => {
        const saved = Number(localStorage.getItem('tcloud:explorer-card-scale'));
        return Number.isFinite(saved) && saved >= 0.5 && saved <= 2 ? saved : 1.0;
    });
    const [sortField, setSortField] = useState<SortField>('name');
    const [sortDirection, setSortDirection] = useState<SortDirection>('asc');
    const [internalDrag, setInternalDrag] = useState<{ fileIds: number[]; label: string } | null>(null);
    const [internalDragTarget, setInternalDragTarget] = useState<string | null>(null);

    useEffect(() => {
        localStorage.setItem('tcloud:explorer-card-scale', String(cardScale));
    }, [cardScale]);
    const dragSensors = useSensors(
        useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
        useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
    );

    const handleSortChange = (field: SortField) => {
        if (field === sortField) {
            setSortDirection((direction) => direction === 'asc' ? 'desc' : 'asc');
            return;
        }
        setSortField(field);
        setSortDirection('asc');
    };
    const [showRemoteUpload, setShowRemoteUpload] = useState(false);
    const [playingFile, setPlayingFile] = useState<TelegramFile | null>(null);
    const [pdfFile, setPdfFile] = useState<TelegramFile | null>(null);
    const [archiveViewFile, setArchiveViewFile] = useState<TelegramFile | null>(null);
    const [shareFile, setShareFile] = useState<TelegramFile | null>(null);
    const [bulkShareLinks, setBulkShareLinks] = useState<Array<{ file: TelegramFile; link: string }> | null>(null);
    const [bulkShareLoading, setBulkShareLoading] = useState(false);
    const [bulkShareCopied, setBulkShareCopied] = useState<Set<string>>(new Set());
    const [previewContextFiles, setPreviewContextFiles] = useState<TelegramFile[]>([]);
    const [previewContextIndex, setPreviewContextIndex] = useState(-1);
const [moveFileTarget, setMoveFileTarget] = useState<TelegramFile | null>(null);

    // Long-hidden windows keep the active folder hot, but release inactive query
    // payloads accumulated while browsing many folders/videos. The bounded local
    // navigation cache and persistent thumbnail cache make this safe to rebuild.
    useEffect(() => {
        let trimTimer: number | null = null;
        const handleBackgroundState = (event: Event) => {
            const hidden = Boolean((event as CustomEvent<{ hidden?: boolean }>).detail?.hidden);
            if (!hidden) {
                if (trimTimer !== null) {
                    window.clearTimeout(trimTimer);
                    trimTimer = null;
                }
                return;
            }
            if (trimTimer !== null) window.clearTimeout(trimTimer);
            trimTimer = window.setTimeout(() => {
                trimTimer = null;
                queryClient.removeQueries({ queryKey: ['files'], type: 'inactive' });
                queryClient.removeQueries({ queryKey: ['video-metadata'], type: 'inactive' });
                queryClient.removeQueries({ queryKey: ['cached-variants'], type: 'inactive' });
            }, 90_000);
        };

        window.addEventListener('tcloud-background-state', handleBackgroundState);
        return () => {
            window.removeEventListener('tcloud-background-state', handleBackgroundState);
            if (trimTimer !== null) window.clearTimeout(trimTimer);
        };
    }, [queryClient]);

    // 12.7.1: cached-first Explorer loading. The local SQLite projection is
    // rendered immediately; the Telegram listing refreshes in the background.
    // This keeps startup/navigation responsive even with large remote folders.
    const remoteRefreshInFlightRef = useRef<Set<string>>(new Set());
    const { data: allFiles = [], isLoading, error } = useQuery({
        queryKey: ['files', activeFolderId],
        queryFn: async () => {
            const indexedState = new Map<number, Pick<TelegramFile, 'sync_status' | 'local_status' | 'is_pinned'>>();
            const memorySnapshot = getFolderNavigationSnapshot(activeFolderId);
            let cachedFiles: TelegramFile[] = memorySnapshot?.files ?? [];

            // 12.9.3: hot folders reopen directly from the bounded RAM LRU. Only hit
            // SQLite when the folder is not already warm; this removes the async gap
            // between clicking a recently used folder and seeing its contents again.
            if (!memorySnapshot) {
                try {
                    const indexed = await listIndexedFiles(activeFolderId);
                    cachedFiles = indexed.map((row) => {
                        const dot = row.name.lastIndexOf('.');
                        const fileExt =
                            dot > 0 && dot < row.name.length - 1
                                ? row.name.slice(dot + 1).toLowerCase()
                                : undefined;

                        const mimeType =
                            fileExt === 'pdf' ? 'application/pdf' :
                            ['jpg', 'jpeg'].includes(fileExt ?? '') ? 'image/jpeg' :
                            fileExt === 'png' ? 'image/png' :
                            fileExt === 'gif' ? 'image/gif' :
                            fileExt === 'webp' ? 'image/webp' :
                            fileExt === 'mp4' ? 'video/mp4' :
                            fileExt === 'webm' ? 'video/webm' :
                            fileExt === 'mkv' ? 'video/x-matroska' :
                            fileExt === 'mp3' ? 'audio/mpeg' :
                            fileExt === 'wav' ? 'audio/wav' :
                            fileExt === 'zip' ? 'application/zip' :
                            undefined;

                        return {
                            id: row.message_id,
                            name: row.name,
                            size: row.size,
                            sizeStr: formatBytes(row.size),
                            mime_type: mimeType,
                            file_ext: fileExt,
                            created_at: row.created_at ?? undefined,
                            type: 'file' as const,
                            folder_id: activeFolderId,
                            sync_status: row.sync_status,
                            local_status: row.local_status,
                            is_pinned: row.is_pinned,
                        };
                    });
                    if (cachedFiles.length > 0) setFolderNavigationSnapshot(activeFolderId, cachedFiles, Date.now() - 6 * 60_000);
                } catch (cacheError) {
                    console.warn('TCloud local index load failed', cacheError);
                }
            }

            for (const file of cachedFiles) {
                indexedState.set(file.id, {
                    sync_status: file.sync_status,
                    local_status: file.local_status,
                    is_pinned: file.is_pinned,
                });
            }
            if (cachedFiles.length > 0) {
                queryClient.setQueryData(['files', activeFolderId], cachedFiles);
            }

            const refreshRemote = async (preserveCachedOnError: boolean): Promise<TelegramFile[]> => {
                const accumulatedFiles = new Map<number, TelegramFile>();
                let flushTimer: ReturnType<typeof setTimeout> | null = null;
                let dirty = false;

                const flush = () => {
                    if (!dirty) return;
                    dirty = false;
                    const nextFiles = Array.from(accumulatedFiles.values());
                    queryClient.setQueryData<TelegramFile[]>(['files', activeFolderId], (current) =>
                        sameFileSnapshot(current, nextFiles) ? current : nextFiles);
                };

                const scheduleFlush = () => {
                    // When cached rows are already visible, do not repaint the entire
                    // Explorer grid for every 500-message Telegram chunk. Accumulate the
                    // background refresh silently and publish the authoritative snapshot
                    // once at the end. First-use folders still get progressive rendering.
                    if (preserveCachedOnError || flushTimer) return;
                    flushTimer = setTimeout(() => {
                        flushTimer = null;
                        flush();
                    }, 400);
                };

                const unlisten = await listen<any>('folder-load-chunk', (event) => {
                    const payload = event.payload;
                    if (payload.folderId === activeFolderId) {
                        const newChunk: TelegramFile[] = payload.files.map((f: any) => ({
                            ...f,
                            sizeStr: formatBytes(f.size),
                            type: (f.icon_type as TelegramFile['type']) || 'file'
                        }));
                        newChunk.forEach((file) => {
                            const state = indexedState.get(file.id);
                            accumulatedFiles.set(file.id, { ...file, ...state });
                        });
                        dirty = true;
                        scheduleFlush();
                    }
                });

                try {
                    await invoke('cmd_get_files', { folderId: activeFolderId, emitChunks: true, collectResults: false });
                    if (flushTimer) {
                        clearTimeout(flushTimer);
                        flushTimer = null;
                    }
                    flush();
                    const finalFiles = Array.from(accumulatedFiles.values());

                    const currentSnapshot =
                        queryClient.getQueryData<TelegramFile[]>(['files', activeFolderId]) ?? [];

                    const remoteChanged =
                        !sameFileSnapshot(currentSnapshot, finalFiles);

                    queryClient.setQueryData<TelegramFile[]>(
                        ['files', activeFolderId],
                        remoteChanged ? finalFiles : currentSnapshot
                    );

                    if (remoteChanged) {
                        setFolderNavigationSnapshot(activeFolderId, finalFiles);
                    }

                        void upsertIndexedFiles(activeFolderId, finalFiles
                            .filter((file) => file.type !== 'folder' && file.name.trim().length > 0)
                            .map((file) => {
                                let indexedCreatedAt: string | null = null;

                                if (file.created_at) {
                                    const numeric = Number(file.created_at);

                                    if (Number.isFinite(numeric) && numeric > 0) {
                                        indexedCreatedAt = String(
                                            numeric < 10_000_000_000
                                                ? numeric
                                                : Math.floor(numeric / 1000)
                                        );
                                    } else {
                                        const parsed = Date.parse(
                                            file.created_at.replace(/ UTC$/i, 'Z')
                                        );

                                        if (!Number.isNaN(parsed)) {
                                            indexedCreatedAt =
                                                String(Math.floor(parsed / 1000));
                                        }
                                    }
                                }

                                return {
                                    message_id: file.id,
                                    name: file.name,
                                    size: file.size,
                                    mime_type: file.mime_type ?? null,
                                    created_at: indexedCreatedAt
                                };
                            }))
                            .catch((indexError) =>
                                console.warn(
                                    'Local index update failed',
                                    indexError
                                )
                            );

                    return finalFiles;
                } catch (remoteError) {
                    if (preserveCachedOnError) {
                        console.warn('TCloud background folder refresh skipped', remoteError);
                        return cachedFiles;
                    }
                    throw remoteError;
                } finally {
                    if (flushTimer) clearTimeout(flushTimer);
                    unlisten();
                }
            };

            // Always leave React Query's loading state from the local SQLite snapshot,
            // even when the snapshot is empty. Telegram refresh then runs after first
            // paint and streams results progressively. This removes the long skeleton
            // screen on first visit to a folder without making remote data stale.
            const refreshKey = activeFolderId === null ? 'saved' : String(activeFolderId);
            if (!remoteRefreshInFlightRef.current.has(refreshKey)) {
                remoteRefreshInFlightRef.current.add(refreshKey);
                const hasCachedSnapshot = cachedFiles.length > 0;
                window.setTimeout(() => {
                    void refreshRemote(hasCachedSnapshot).finally(() => {
                        remoteRefreshInFlightRef.current.delete(refreshKey);
                    });
                }, memorySnapshot ? 900 : hasCachedSnapshot ? 350 : 40);
            }
            return cachedFiles;
        },
        staleTime: 5 * 60_000,
        // Cold React Query entries can be released earlier because the separate
        // navigation LRU is bounded and keeps only the hottest folders.
        gcTime: 12 * 60_000,
        initialData: () => getFolderNavigationSnapshot(activeFolderId)?.files,
        initialDataUpdatedAt: () => getFolderNavigationSnapshot(activeFolderId)?.updatedAt,
        placeholderData: [],
        refetchOnWindowFocus: false,
        enabled: !!store,
    });

    // Keep uploads/moves reflected in the hot-folder cache. The array and file
    // objects are shared with React Query, so this adds no second copy of the rows.
    useEffect(() => {
        if (allFiles.length > 0) {
            const existingSnapshot = getFolderNavigationSnapshot(activeFolderId);
            setFolderNavigationSnapshot(activeFolderId, allFiles, existingSnapshot?.updatedAt ?? Date.now());
        }
    }, [activeFolderId, allFiles]);

    const folderWarmTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const warmFolderForNavigation = useCallback((folderId: number) => {
        if (folderId === activeFolderId) return;
        if (folderWarmTimerRef.current) clearTimeout(folderWarmTimerRef.current);
        folderWarmTimerRef.current = setTimeout(() => {
            folderWarmTimerRef.current = null;
            const existing = queryClient.getQueryData<TelegramFile[]>(['files', folderId]);
            if (existing?.length) {
                setFolderNavigationSnapshot(folderId, existing);
                return;
            }
            void warmFolderNavigationSnapshot(folderId).then((files) => {
                if (files.length && !queryClient.getQueryData(['files', folderId])) {
                    queryClient.setQueryData(['files', folderId], files, { updatedAt: Date.now() - 6 * 60_000 });
                }
            }).catch(() => undefined);
        }, 120);
    }, [activeFolderId, queryClient]);

    useEffect(() => () => {
        if (folderWarmTimerRef.current) clearTimeout(folderWarmTimerRef.current);
    }, []);

    // Warm one adjacent folder from the local index only after the current view is
    // settled. This makes the common "next folder" action instant without causing
    // speculative Telegram traffic.
    useEffect(() => {
        if (activeFolderId === null || allFiles.length === 0) return;
        const orderedIds: number[] = [];
        const seen = new Set<number>();
        for (const id of [
            ...forumTree.flatMap((forum) => forum.topics.map((topic) => topic.id)),
            ...folders.map((folder) => folder.id),
        ]) {
            if (id >= 0 && !seen.has(id)) {
                seen.add(id);
                orderedIds.push(id);
            }
        }
        const index = orderedIds.indexOf(activeFolderId);
        if (index < 0) return;
        const nextFolderId = orderedIds[index + 1] ?? orderedIds[index - 1];
        if (nextFolderId === undefined || nextFolderId === activeFolderId) return;

        const timer = window.setTimeout(() => {
            if (!isTCloudUiBackgroundReady()) return;
            if (queryClient.getQueryData<TelegramFile[]>(['files', nextFolderId])?.length) return;
            void warmFolderNavigationSnapshot(nextFolderId).then((files) => {
                if (files.length && !queryClient.getQueryData(['files', nextFolderId])) {
                    queryClient.setQueryData(['files', nextFolderId], files, { updatedAt: Date.now() - 6 * 60_000 });
                }
            }).catch(() => undefined);
        }, 1400);
        return () => window.clearTimeout(timer);
    }, [activeFolderId, allFiles.length, folders, forumTree, queryClient]);

    useEffect(() => {
        let alive = true;
        const refreshTrashIds = async () => {
            try {
                const items = await listTrash();
                if (alive) setTrashIds(new Set(items.map((item) => item.message_id)));
            } catch {
                if (alive) setTrashIds(new Set());
            }
        };
        void refreshTrashIds();
        const handler = () => void refreshTrashIds();
        window.addEventListener('tcloud-trash-changed', handler);
        return () => { alive = false; window.removeEventListener('tcloud-trash-changed', handler); };
    }, []);

    const visibleAllFiles = useMemo(
        () => allFiles.filter((file) => file.type === 'folder' || !trashIds.has(file.id)),
        [allFiles, trashIds],
    );

    useEffect(() => {
        if (!isConnected) return;
        const purgeExpiredTrash = async () => {
            if (document.visibilityState === 'visible' && !isTCloudUiBackgroundReady(1200)) return;
            const days = Number(localStorage.getItem('tcloud:trash-retention-days') || '30');
            if (!Number.isFinite(days) || days <= 0) return;
            const threshold = Math.floor(Date.now() / 1000) - days * 86400;
            try {
                const items = await listTrash();
                for (const item of items.filter((entry) => entry.deleted_at <= threshold)) {
                    try {
                        await purgeTrash(item.id);
                    } catch (error) {
                        console.warn('Expired trash purge failed', item.name, error);
                    }
                }
                window.dispatchEvent(new Event('tcloud-trash-changed'));
            } catch (error) {
                console.warn('Trash retention check failed', error);
            }
        };
        void purgeExpiredTrash();
        const timer = window.setInterval(() => void purgeExpiredTrash(), 6 * 60 * 60 * 1000);
        return () => window.clearInterval(timer);
    }, [isConnected]);

    const deferredSearchTerm = useDeferredValue(searchTerm);
    const advancedSearch = useMemo(() => {
        const raw = parseNaturalTCloudSearch(deferredSearchTerm);
        const tokens = raw.match(/(?:type|size|before|after):[^\s]+/gi) || [];
        const plain = raw.replace(/(?:type|size|before|after):[^\s]+/gi, ' ').replace(/\s+/g, ' ').trim();
        let type: string | null = null;
        let size: { op: '>' | '<'; bytes: number } | null = null;
        let before: Date | null = null;
        let after: Date | null = null;
        const parseSize = (value: string) => {
            const match = value.match(/^([<>])?([0-9.]+)(kb|mb|gb|tb|b)?$/i);
            if (!match) return null;
            const unit = (match[3] || 'b').toLowerCase();
            const factor = unit === 'tb' ? 1024 ** 4 : unit === 'gb' ? 1024 ** 3 : unit === 'mb' ? 1024 ** 2 : unit === 'kb' ? 1024 : 1;
            return { op: (match[1] === '<' ? '<' : '>') as '>' | '<', bytes: Number(match[2]) * factor };
        };
        for (const token of tokens) {
            const [key, value = ''] = token.split(':', 2);
            if (key.toLowerCase() === 'type') type = value.toLowerCase();
            if (key.toLowerCase() === 'size') size = parseSize(value);
            if (key.toLowerCase() === 'before') { const d = new Date(value); if (!Number.isNaN(d.getTime())) before = d; }
            if (key.toLowerCase() === 'after') { const d = new Date(value); if (!Number.isNaN(d.getTime())) after = d; }
        }
        return { plain, type, size, before, after, hasTokens: tokens.length > 0 };
    }, [deferredSearchTerm]);
    const normalizedSearchTerm = useMemo(() => advancedSearch.plain.toLocaleLowerCase(), [advancedSearch.plain]);
    const searchBaseFiles = useMemo(() => {
        if (normalizedSearchTerm.length > 2) return searchResults;
        if (normalizedSearchTerm.length === 0) return visibleAllFiles;
        return visibleAllFiles.filter((file: TelegramFile) => file.name.toLocaleLowerCase().includes(normalizedSearchTerm));
    }, [visibleAllFiles, normalizedSearchTerm, searchResults]);

    // Explorer collections are virtual views. Reuse every folder that is already
    // warm in React Query instead of opening a separate tools window or starting
    // speculative Telegram scans just to build Galeria/Música/Cofre.
    const cachedLibraryFiles = useMemo(() => {
        const merged = new Map<string, TelegramFile>();
        for (const [, value] of queryClient.getQueriesData<TelegramFile[]>({ queryKey: ['files'] })) {
            for (const file of value ?? []) {
                if (file.type !== 'folder' && !trashIds.has(file.id)) merged.set(`${file.folder_id ?? 0}:${file.id}`, file);
            }
        }
        for (const file of visibleAllFiles) {
            if (file.type !== 'folder' && !trashIds.has(file.id)) merged.set(`${file.folder_id ?? 0}:${file.id}`, file);
        }
        return Array.from(merged.values());
    }, [queryClient, visibleAllFiles, trashIds, rootView]);

    const collectionSourceFiles = useMemo(() => {
        const isCollection = ['recent', 'favorites', 'problems', 'gallery', 'music', 'offline', 'vault'].includes(rootView);
        if (activeFolderId !== null || !isCollection) return searchBaseFiles;
        if (!normalizedSearchTerm) return cachedLibraryFiles;
        return cachedLibraryFiles.filter((file) => file.name.toLocaleLowerCase('pt-BR').includes(normalizedSearchTerm));
    }, [activeFolderId, cachedLibraryFiles, normalizedSearchTerm, rootView, searchBaseFiles]);

    const explorerBaseFiles = useMemo(() => {
        if (activeFolderId !== null) return collectionSourceFiles;
        if (rootView === 'gallery') {
            return collectionSourceFiles.filter((file) => file.type !== 'folder' && /\.(jpe?g|png|webp|gif|bmp|heic|heif|avif|mp4|mov|m4v|webm|mkv)$/i.test(file.name));
        }
        if (rootView === 'music') {
            return collectionSourceFiles.filter((file) => file.type !== 'folder' && /\.(mp3|m4a|aac|flac|wav|ogg|opus|wma)$/i.test(file.name));
        }
        if (rootView === 'offline') {
            return collectionSourceFiles.filter((file) => {
                if (file.type === 'folder') return false;
                const local = (file.local_status || '').toLowerCase();
                return Boolean(file.is_pinned) || ['cached', 'offline', 'local', 'available'].some((value) => local.includes(value));
            });
        }
        if (rootView === 'vault') {
            return collectionSourceFiles.filter((file) => file.type !== 'folder' && ['vault', 'vault_and_passphrase'].includes(file.encryption?.protection_mode || ''));
        }
        if (normalizedSearchTerm.length > 0) return collectionSourceFiles;
        if (rootView === 'recent') {
            return [...collectionSourceFiles]
                .filter((file) => {
                    if (file.type === 'folder') return false;

                    const name = (file.name || '').trim();

                    // Arquivos usados apenas pelos testes internos do TCloud.
                    if (isTCloudInternalTestFile(name)) return false;

                    // Cópias históricas geradas pelo mecanismo antigo de conflitos.
                    // Continuam disponíveis em Mensagens Salvas, mas não poluem Recentes.
                    if (/conflito deste computador/i.test(name)) return false;

                    return true;
                })
                .sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''))
                .slice(0, 250);
        }
        if (rootView === 'favorites') {
            const favorites = new Set(favoriteIds);
            return collectionSourceFiles.filter((file) => file.type !== 'folder' && favorites.has(favoriteKey(file.id, file.folder_id)));
        }
        if (rootView === 'problems') {
            return collectionSourceFiles.filter((file) => {
                const sync = (file.sync_status || '').toLowerCase();
                return ['failed', 'error', 'conflict'].some((value) => sync.includes(value));
            });
        }
        return collectionSourceFiles;
    }, [activeFolderId, favoriteIds, normalizedSearchTerm.length, rootView, collectionSourceFiles]);

    const getFileExtension = useCallback((name: string) => {
        const cleanName = name.split('?')[0].split('#')[0];
        return cleanName.includes('.') ? cleanName.split('.').pop()?.toLocaleLowerCase() || '' : '';
    }, []);

    const parseFileDate = useCallback((value?: string) => {
        if (!value) return null;
        const numeric = Number(value);
        if (Number.isFinite(numeric) && numeric > 0) {
            const milliseconds = numeric < 10_000_000_000 ? numeric * 1000 : numeric;
            const date = new Date(milliseconds);
            return Number.isNaN(date.getTime()) ? null : date;
        }

        const normalized = value
            .replace(/\sUTC$/i, 'Z')
            .replace(/^(\d{4}-\d{2}-\d{2})\s(\d{2}:\d{2}:\d{2})Z$/, '$1T$2Z');
        const date = new Date(normalized);
        return Number.isNaN(date.getTime()) ? null : date;
    }, []);

    const displayedFiles = useMemo(() => explorerBaseFiles.filter((file: TelegramFile) => {
        if (file.type === 'folder') {
            return fileFilter === 'all' && dateFilter === 'any';
        }

        const ext = getFileExtension(file.name);
        if (advancedSearch.type) {
            const groups: Record<string, string[]> = {
                image: ['jpg','jpeg','png','gif','webp','bmp','heic','avif'],
                images: ['jpg','jpeg','png','gif','webp','bmp','heic','avif'],
                video: ['mp4','mkv','mov','avi','webm','m4v','mpeg','mpg'],
                videos: ['mp4','mkv','mov','avi','webm','m4v','mpeg','mpg'],
                audio: ['mp3','wav','m4a','aac','flac','ogg','opus','wma'],
                document: ['pdf','doc','docx','xls','xlsx','ppt','pptx','txt','csv','odt','ods','odp','rtf'],
                documents: ['pdf','doc','docx','xls','xlsx','ppt','pptx','txt','csv','odt','ods','odp','rtf'],
                archive: ['zip','rar','7z','tar','gz','bz2','xz','tgz'],
            };
            const accepted = groups[advancedSearch.type] || [advancedSearch.type.replace(/^\./, '')];
            if (!accepted.includes(ext)) return false;
        }
        if (advancedSearch.size) {
            if (advancedSearch.size.op === '>' && file.size <= advancedSearch.size.bytes) return false;
            if (advancedSearch.size.op === '<' && file.size >= advancedSearch.size.bytes) return false;
        }
        if (advancedSearch.before || advancedSearch.after) {
            const date = parseFileDate(file.created_at);
            if (!date) return false;
            if (advancedSearch.before && date >= advancedSearch.before) return false;
            if (advancedSearch.after && date <= advancedSearch.after) return false;
        }
        const matchesType = (() => {
            switch (fileFilter) {
                case 'images':
                    return ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'heic', 'avif'].includes(ext);
                case 'videos':
                    return ['mp4', 'mkv', 'mov', 'avi', 'webm', 'm4v', 'mpeg', 'mpg'].includes(ext);
                case 'audio':
                    return ['mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'opus', 'wma'].includes(ext);
                case 'documents':
                    return ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'csv', 'odt', 'ods', 'odp', 'rtf'].includes(ext);
                case 'archives':
                    return ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'tgz'].includes(ext);
                default:
                    return true;
            }
        })();

        if (!matchesType) return false;
        if (dateFilter === 'any') return true;

        const created = parseFileDate(file.created_at);
        if (!created) return false;

        const now = new Date();
        if (dateFilter === 'today') {
            return created.getFullYear() === now.getFullYear()
                && created.getMonth() === now.getMonth()
                && created.getDate() === now.getDate();
        }

        const ageMs = now.getTime() - created.getTime();
        const days = dateFilter === '7days' ? 7 : 30;
        return ageMs >= 0 && ageMs <= days * 24 * 60 * 60 * 1000;
    }), [explorerBaseFiles, fileFilter, dateFilter, getFileExtension, parseFileDate, advancedSearch]);

    const activeFilterCount = (fileFilter !== 'all' ? 1 : 0) + (dateFilter !== 'any' ? 1 : 0);
    const hasActiveSearch = normalizedSearchTerm.length > 0;
    const showForumOverview =
        selectedForum !== null
        && activeFolderId === null
        && !hasActiveSearch
        && activeFilterCount === 0;
    const showHomeDashboard =
        !showForumOverview
        && activeFolderId === null
        && rootView === 'dashboard'
        && !hasActiveSearch
        && activeFilterCount === 0;
    const showUtilityWorkspace = activeFolderId === null && (rootView === 'trash' || rootView === 'sync');
    const showFileExplorer =
        !showForumOverview
        && !showUtilityWorkspace
        && (
            activeFolderId !== null
            || rootView !== 'dashboard'
            || hasActiveSearch
            || activeFilterCount > 0
        );

        function isTCloudInternalTestFile(name: string): boolean {
        const normalized = (name || '').trim().toLowerCase();

        return (
            normalized.startsWith('benchmark-tcloud-') ||
            normalized.startsWith('benchmark8-tcloud-') ||
            normalized.startsWith('final-sync-test-') ||
            normalized.startsWith('teste-tcloud-novo') ||

            // Cópias históricas criadas pela antiga política automática
            // "keep_both". Elas continuam acessíveis em Mensagens Salvas,
            // mas não devem dominar Início/Recentes.
            normalized.includes('conflito deste computador') ||
            normalized.includes('conflito_deste_computador')
        );
    }

    const dashboardCleanFiles = useMemo(
        () =>
            visibleAllFiles.filter(
                (file) =>
                    file.type === 'folder' ||
                    !isTCloudInternalTestFile(file.name)
            ),
        [visibleAllFiles],
    );
const dashboardRecentFiles = useMemo(() =>
        [...dashboardCleanFiles]
            .filter((file) => file.type !== 'folder' && file.name.trim().length > 0)
            .sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''))
            .slice(0, 6),
        [dashboardCleanFiles]
    );

    const dashboardFavoriteFiles = useMemo(() => {
        const favorites = new Set(favoriteIds);
        return dashboardCleanFiles
            .filter((file) => file.type !== 'folder' && favorites.has(favoriteKey(file.id, file.folder_id)))
            .slice(0, 5);
    }, [dashboardCleanFiles, favoriteIds]);

    const dashboardMainFiles = useMemo(() =>
        [...dashboardCleanFiles]
            .filter((file) => file.type !== 'folder' && file.name.trim().length > 0)
            .sort((a, b) => (b.created_at || '').localeCompare(a.created_at || ''))
            .slice(0, 12),
        [dashboardCleanFiles]
    );

    const { data: explorerConnection } = useQuery({
        queryKey: ['cfapi-status'],
        queryFn: getCfapiStatus,
        // Healthy Explorer state changes slowly; reconnect/problems are checked
        // aggressively without polling the native layer every few seconds forever.
        refetchInterval: (query) => {
            const status = query.state.data as Awaited<ReturnType<typeof getCfapiStatus>> | undefined;
            if (!status) return 6_000;
            if (!status.registered) return 60_000;
            return status.provider_connected ? 30_000 : 6_000;
        },
        refetchIntervalInBackground: false,
        enabled: !!store,
    });

    const { data: syncOverview } = useQuery({
        queryKey: ['tcloud-sync-overview'],
        queryFn: getTCloudSyncOverview,
        refetchInterval: (query) => {
            const overview = query.state.data as Awaited<ReturnType<typeof getTCloudSyncOverview>> | undefined;
            const active = (overview?.pending ?? 0) + (overview?.running ?? 0);
            if (active > 0) return 4_000;
            return (overview?.failed ?? 0) > 0 ? 15_000 : 20_000;
        },
        refetchIntervalInBackground: false,
        enabled: !!store,
    });

    const syncingCount = (syncOverview?.pending ?? 0) + (syncOverview?.running ?? 0);
    const problemCount = syncOverview?.failed ?? 0;
    const historicalProblemCount = syncOverview?.historical_failed ?? 0;

    const { data: bandwidth } = useQuery({
        queryKey: ['bandwidth'],
        queryFn: () => invoke<BandwidthStats>('cmd_get_bandwidth'),
        refetchInterval: () => {
            if (syncingCount > 0) return 5_000;
            return 60_000;
        },
        refetchIntervalInBackground: false,
        enabled: !!store
    });


    const { uploadQueue, setUploadQueue, handleManualUpload, handleFolderUpload, handleDropUpload, handleUrlUpload, cancelAll: cancelUploads, cancelItem: cancelUploadItem, retryItem: retryUploadItem, setItemPriority: setUploadPriority } = useFileUpload(activeFolderId, store);

    const handleUploadFromCurrentView = useCallback(async () => {
        // Do not silently fall back to a hidden/default destination. The Explorer
        // model is explicit: files go to the folder the user is currently in.
        if (activeFolderId === null && rootView !== 'saved') {
            if (selectedForum) {
                toast.info(`Abra uma subpasta de ${selectedForum.name} para enviar o arquivo.`);
            } else {
                toast.info('Abra a pasta onde deseja salvar o arquivo antes de enviar.');
            }
            return;
        }
        await handleManualUpload();
    }, [activeFolderId, rootView, selectedForum, handleManualUpload]);

    const handleFolderUploadFromCurrentView = useCallback(async () => {
        if (activeFolderId === null && rootView !== 'saved') {
            toast.info('Abra a pasta onde deseja enviar a pasta antes de continuar.');
            return;
        }
        await handleFolderUpload();
    }, [activeFolderId, rootView, handleFolderUpload]);

    const handleDropFromCurrentView = useCallback(async (paths: string[]) => {
        if (activeFolderId === null && rootView !== 'saved') {
            if (selectedForum) {
                toast.info(`Abra uma subpasta de ${selectedForum.name} antes de soltar arquivos aqui.`);
            } else {
                toast.info('Abra a pasta onde deseja salvar antes de soltar arquivos.');
            }
            return { queued: 0, rejected: [], cancelled: true };
        }
        return handleDropUpload(paths);
    }, [activeFolderId, rootView, selectedForum, handleDropUpload]);

    const { downloadQueue, queueDownload, queueBulkDownload, clearFinished: clearDownloads, cancelAll: cancelDownloads, cancelItem: cancelDownloadItem, retryItem: retryDownloadItem } = useFileDownload(store);

    const {
        handleDelete, handleBulkDelete, handleBulkDownload,
        handleBulkMove, handleDownloadFolder, handleGlobalSearch

    } = useFileOperations(activeFolderId, selectedIds, setSelectedIds, displayedFiles, queueBulkDownload);

    // Bulk share: generate links for all selected non-folder files
    const handleBulkShare = useCallback(async () => {
        const shareFiles = displayedFiles.filter(f => selectedIdSet.has(f.id) && f.type !== 'folder');
        if (shareFiles.length === 0) {
            toast.info('No shareable files selected (folders cannot be shared)');
            return;
        }
        setBulkShareLinks([]);
        setBulkShareLoading(true);
        setBulkShareCopied(new Set());
        try {
            const results = await Promise.all(
                shareFiles.map(async (file) => {
                    try {
                        const info = await invoke<ShareInfo>('cmd_create_share', {
                            folderId: file.folder_id ?? null,
                            messageId: file.id,
                            fileName: file.name,
                            fileSize: file.size,
                            password: null,
                            expiryHours: 24,
                            maxDownloads: null,
                        });
                        return { file, link: info.link };
                    } catch (e) {
                        toast.error(`Failed to share ${file.name}: ${e}`);
                        return null;
                    }
                })
            );
            const valid = results.filter((r): r is { file: TelegramFile; link: string } => r !== null);
            if (valid.length > 0) {
                setBulkShareLinks(valid);
                setSelectedIds([]);
            } else {
                setBulkShareLinks(null);
                toast.error('Failed to generate any share links');
            }
        } finally {
            setBulkShareLoading(false);
        }
    }, [displayedFiles, selectedIds]);

    const handleCopyBulkLink = useCallback((link: string) => {
        navigator.clipboard.writeText(link);
        setBulkShareCopied(prev => new Set(prev).add(link));
        setTimeout(() => setBulkShareCopied(prev => {
            const next = new Set(prev);
            next.delete(link);
            return next;
        }), 2000);
    }, []);


    const handleSelectAll = useCallback(() => {
        setSelectedIds(displayedFiles.map(f => f.id));
    }, [displayedFiles]);

    const handleExplorerDelete = useCallback(async (id: number) => {
        const item = displayedFiles.find((file) => file.id === id);
        if (item?.type === 'folder') {
            await handleFolderDelete(item.id, item.name);
            setSelectedIds((current) => current.filter((candidate) => candidate !== id));
            return;
        }
        await handleDelete(id);
    }, [displayedFiles, handleFolderDelete, handleDelete]);

    const handleKeyboardDelete = useCallback(async () => {
        if (selectedIds.length === 0) return;
        const selected = displayedFiles.filter((file) => selectedIdSet.has(file.id));
        const foldersToDelete = selected.filter((file) => file.type === 'folder');
        const filesToDelete = selected.filter((file) => file.type !== 'folder' && file.name.trim().length > 0);
        if (filesToDelete.length > 0) await handleBulkDelete();
        for (const folder of foldersToDelete) {
            await handleFolderDelete(folder.id, folder.name);
        }
        setSelectedIds([]);
    }, [selectedIds, displayedFiles, handleBulkDelete, handleFolderDelete]);

    const handleEscape = useCallback(() => {
        lastClickedIndexRef.current = -1;
        setSelectedIds([]);
        setSearchTerm("");
        setPreviewFile(null);
        setPlayingFile(null);
        setPdfFile(null);
        setArchiveViewFile(null);
    }, []);

    const handleFocusSearch = useCallback(() => {
        const searchInput = document.querySelector('.tcloud-explorer-search input') as HTMLInputElement;
        if (searchInput) {
            searchInput.focus();
            searchInput.select();
        }
    }, []);

    const handleEnter = useCallback(() => {
        if (selectedIds.length === 1) {
            const selected = displayedFiles.find(f => f.id === selectedIds[0]);
            if (selected) {
                if (selected.type === 'folder') {
                    setActiveFolderId(selected.id);
                } else {
                    handlePreview(selected, displayedFiles);
                }
            }
        }
    }, [selectedIds, displayedFiles, setActiveFolderId]);


    // Favorites remain synchronized independently from the CFAPI Explorer projection.
    useEffect(() => subscribeFavorites(setFavoriteIds), []);
    useEffect(() => {
        if (!isConnected) return;
        void syncFavoritesFromTelegram().catch(() => {});
    }, [isConnected]);



    useEffect(() => {
        void getTCloudSetupState()
            .then((state) => {
                if (!state.completed && state.folder_count === 0) {
                    setShowTCloudSetup(true);
                }
            })
            .catch(() => {
                // Setup can still be opened manually later.
            });
    }, []);

    useEffect(() => {
        lastClickedIndexRef.current = -1;
        setSelectedIds([]);
        setShowMoveModal(false);
        setSearchTerm("");
        setSearchResults([]);
        setFileFilter('all');
        setDateFilter('any');
        setPreviewFile(null);
        setPlayingFile(null);
        setPdfFile(null);
        setPreviewContextFiles([]);
        setPreviewContextIndex(-1);
        setArchiveViewFile(null);
    }, [activeFolderId]);


    useEffect(() => {
        const remoteTerm = advancedSearch.plain.trim();
        const requestSeq = ++searchRequestSeqRef.current;
        if (remoteTerm.length <= 2) {
            setSearchResults([]);
            setIsSearching(false);
            return;
        }

        let cancelled = false;
        let localTimer: number | null = null;
        let remoteTimer: number | null = null;
        let localHits: Awaited<ReturnType<typeof searchIndexedContent>> = [];
        let latestRemoteResults: TelegramFile[] = [];
        const normalized = remoteTerm.toLocaleLowerCase();
        const visibleByKey = new Map<string, TelegramFile>(
            visibleAllFiles.map((file): [string, TelegramFile] => [
                `${file.folder_id ?? 0}:${file.id}`,
                file,
            ])
        );

        const mergeResults = (remoteResults: TelegramFile[]) => {
            const merged = new Map<string, TelegramFile>();

            // The SQLite/content index is the fast path. It can render matches from
            // previously visited folders without waiting for Telegram/global search.
            for (const hit of localHits) {
                const messageId = hit.telegram_message_id;
                if (typeof messageId !== 'number' || trashIds.has(messageId)) continue;
                const key = `${hit.telegram_chat_id ?? 0}:${messageId}`;
                const richer = visibleByKey.get(key);
                merged.set(key, richer ?? {
                    id: messageId,
                    name: hit.name,
                    size: hit.size,
                    sizeStr: formatBytes(hit.size),
                    type: 'file',
                    folder_id: hit.telegram_chat_id ?? null,
                });
            }

            // Keep immediate in-folder name matches even if the content index is still
            // warming. Remote results override local fallback rows with richer metadata.
            for (const file of visibleAllFiles) {
                if (trashIds.has(file.id)) continue;
                if (!file.name.toLocaleLowerCase().includes(normalized)) continue;
                merged.set(`${file.folder_id ?? 0}:${file.id}`, file);
            }
            for (const file of remoteResults) {
                if (trashIds.has(file.id)) continue;
                merged.set(`${file.folder_id ?? 0}:${file.id}`, file);
            }
            return [...merged.values()];
        };

        const runLocalSearch = async () => {
            setIsSearching(true);
            try {
                localHits = activeFolderId === null ? await searchIndexedContent(remoteTerm).catch(() => []) : [];
                if (cancelled || searchRequestSeqRef.current !== requestSeq) return;
                setSearchResults(mergeResults(latestRemoteResults));
            } finally {
                if (!cancelled && searchRequestSeqRef.current === requestSeq) {
                    setIsSearching(false);
                }
            }
        };

        const runRemoteSearch = async () => {
            if (cancelled || searchRequestSeqRef.current !== requestSeq) return;
            if (!isTCloudUiBackgroundReady(0, 700)) {
                remoteTimer = window.setTimeout(() => { void runRemoteSearch(); }, 260);
                return;
            }
            try {
                if (activeFolderId !== null) { latestRemoteResults = []; setSearchResults(mergeResults([])); return; }
                const remoteResults = await handleGlobalSearch(remoteTerm);
                if (cancelled || searchRequestSeqRef.current !== requestSeq) return;
                latestRemoteResults = remoteResults;
                setSearchResults(mergeResults(latestRemoteResults));
            } catch {
                // Local results remain usable while Telegram is offline/slow.
            }
        };

        localTimer = window.setTimeout(() => { void runLocalSearch(); }, 160);
        // Telegram search is deliberately delayed until the user has stopped typing
        // and the UI is idle. This avoids a network/database burst for every pause.
        remoteTimer = window.setTimeout(() => { void runRemoteSearch(); }, 900);

        return () => {
            cancelled = true;
            if (localTimer !== null) window.clearTimeout(localTimer);
            if (remoteTimer !== null) window.clearTimeout(remoteTimer);
        };
    }, [advancedSearch.plain, handleGlobalSearch, trashIds, visibleAllFiles, activeFolderId]);




    const lastClickedIndexRef = useRef<number>(-1);

    const clearSelection = useCallback(() => {
        lastClickedIndexRef.current = -1;
        setSelectedIds([]);
        setShowDetailsPanel(false);
    }, []);

    const handleFileClick = (e: MouseEvent, id: number, orderedFiles: TelegramFile[] = []) => {
        e.stopPropagation();
        const filesSource = orderedFiles.length > 0 ? orderedFiles : displayedFiles;
        const currentIndex = filesSource.findIndex(f => f.id === id);

        if (e.shiftKey && lastClickedIndexRef.current >= 0) {
            // Shift+Click: range select from last clicked to current
            const start = Math.min(lastClickedIndexRef.current, currentIndex);
            const end = Math.max(lastClickedIndexRef.current, currentIndex);
            const rangeIds = filesSource.slice(start, end + 1).map(f => f.id);
            setSelectedIds(rangeIds);
        } else if (e.metaKey || e.ctrlKey) {
            // Ctrl/Cmd+Click: toggle individual file
            lastClickedIndexRef.current = currentIndex;
            setSelectedIds(ids => ids.includes(id) ? ids.filter(i => i !== id) : [...ids, id]);
        } else {
            // Plain click: select single file
            lastClickedIndexRef.current = currentIndex;
            setSelectedIds([id]);
        }
    }

    const handleToggleSelection = useCallback((id: number) => {
        setSelectedIds(ids => ids.includes(id) ? ids.filter(i => i !== id) : [...ids, id]);
    }, []);

    const handleFileMove = useCallback((file: TelegramFile) => {
        setMoveFileTarget(file);
        setShowMoveModal(true);
    }, []);

    const beginCreateFolder = useCallback(() => {
        clearSelection();
        const existingNames = selectedForum && activeFolderId === null
            ? selectedForum.topics.map((topic) => topic.name)
            : activeFolderId === null && rootView === 'dashboard'
                ? forumTree.map((forum) => forum.name)
                : displayedFiles.filter((file) => file.type === 'folder').map((file) => file.name);
        setInlineEditor({ mode: 'create', value: windowsLikeFolderName('Nova pasta', existingNames) });
    }, [clearSelection, selectedForum, activeFolderId, rootView, forumTree, displayedFiles]);

    const handleRename = useCallback((file: TelegramFile) => {
        setInlineEditor({ mode: 'rename', value: file.name, target: file });
    }, []);

    const submitInlineEditor = useCallback(async () => {
        if (!inlineEditor || inlineEditorBusy) return;
        const requestedName = inlineEditor.value.trim();
        const newName = requestedName.replace(/[. ]+$/g, '');

        if (!newName) {
            toast.error('Digite um nome v?lido.');
            return;
        }

        if (requestedName !== newName) {
            toast.info('Ponto ou espa?o no fim do nome foi removido para compatibilidade com o Windows.');
            setInlineEditor(current =>
                current ? { ...current, value: newName } : current
            );
        }
        setInlineEditorBusy(true);
        try {
            if (inlineEditor.mode === 'create') {
                await createFolderInCurrentContext(newName);
            } else if (inlineEditor.target.type === 'folder') {
                await handleFolderRename(inlineEditor.target.id, newName);

                await Promise.all([
                    queryClient.invalidateQueries({ queryKey: ['folders'] }),
                    queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] }),
                ]);

                await queryClient.refetchQueries({
                    queryKey: ['tcloud-forum-tree']
                });
            } else {
                const folderId = inlineEditor.target.folder_id ?? activeFolderId;
                await invoke('cmd_rename_file', {
                    messageId: inlineEditor.target.id,
                    folderId,
                    newName,
                });
                queryClient.invalidateQueries({ queryKey: ['files', folderId] });
                queryClient.invalidateQueries({ queryKey: ['files', activeFolderId] });
                toast.success(`Renomeado para "${newName}".`);
            }
            setInlineEditor(null);
        } catch (error) {
            toast.error(`Não foi possível concluir: ${String(error)}`);
        } finally {
            setInlineEditorBusy(false);
        }
    }, [inlineEditor, inlineEditorBusy, createFolderInCurrentContext, handleFolderRename, queryClient, activeFolderId]);

    const handleKeyboardDownload = useCallback(() => {
        if (selectedIds.length > 0) {
            handleBulkDownload();
        }
    }, [selectedIds, handleBulkDownload]);

    const handleKeyboardShare = useCallback(() => {
        if (selectedIds.length > 0) {
            handleBulkShare();
        }
    }, [selectedIds, handleBulkShare]);

    const handleKeyboardRename = useCallback(() => {
        if (selectedIds.length !== 1) return;
        const selected = displayedFiles.find(f => f.id === selectedIds[0]);
        if (!selected) return;
        handleRename(selected);
    }, [selectedIds, displayedFiles, handleRename]);

    const handleKeyboardCreateFolder = useCallback(() => {
        beginCreateFolder();
    }, [beginCreateFolder]);

    const handleKeyboardGoUp = useCallback(() => {
        if (activeFolderId !== null) {
            setActiveFolderId(null);
            return;
        }
        if (rootView !== 'dashboard') setRootView('dashboard');
    }, [activeFolderId, rootView, setActiveFolderId]);

    const captureClipboardSelection = useCallback((mode: ExplorerClipboardMode) => {
        const selected = displayedFiles.filter((file) => selectedIdSet.has(file.id));
        const filesOnly = selected.filter((file) => file.type !== 'folder' && file.name.trim().length > 0);
        const foldersSelected = selected.length - filesOnly.length;
        if (filesOnly.length === 0) {
            if (foldersSelected > 0) toast.info('Copiar/recortar pastas completas será habilitado na próxima etapa. Para mover uma pasta agora, use Mover.');
            return;
        }
        const items = filesOnly.map((file) => ({
            id: file.id,
            name: file.name,
            sourceFolderId: file.folder_id ?? activeFolderId,
        }));
        setExplorerClipboard({ mode, items });
        if (foldersSelected > 0) toast.info(`${foldersSelected} pasta(s) não entraram na área de transferência.`);
        toast.success(`${items.length} arquivo${items.length === 1 ? '' : 's'} ${mode === 'cut' ? 'recortado' : 'copiado'}${items.length === 1 ? '' : 's'}.`);
    }, [displayedFiles, selectedIds, activeFolderId]);

    const handleExplorerCopy = useCallback(() => captureClipboardSelection('copy'), [captureClipboardSelection]);
    const handleExplorerCut = useCallback(() => captureClipboardSelection('cut'), [captureClipboardSelection]);

    const handleExplorerPaste = useCallback(async () => {
        if (!explorerClipboard || explorerClipboard.items.length === 0) return;
        if (activeFolderId === null && rootView !== 'saved') {
            toast.info('Abra a pasta de destino antes de colar.');
            return;
        }
        const targetFolderId = activeFolderId;
        const groups = new Map<string, { sourceFolderId: number | null; ids: number[] }>();
        for (const item of explorerClipboard.items) {
            const key = item.sourceFolderId === null ? 'home' : String(item.sourceFolderId);
            const existing = groups.get(key) ?? { sourceFolderId: item.sourceFolderId, ids: [] };
            existing.ids.push(item.id);
            groups.set(key, existing);
        }
        try {
            for (const group of groups.values()) {
                if (explorerClipboard.mode === 'copy') {
                    await invoke('cmd_copy_files', {
                        messageIds: group.ids,
                        sourceFolderId: group.sourceFolderId,
                        targetFolderId,
                    });
                } else {
                    await invoke('cmd_move_files', {
                        messageIds: group.ids,
                        sourceFolderId: group.sourceFolderId,
                        targetFolderId,
                    });
                }
                queryClient.invalidateQueries({ queryKey: ['files', group.sourceFolderId] });
            }
            queryClient.invalidateQueries({ queryKey: ['files', targetFolderId] });
            const count = explorerClipboard.items.length;
            toast.success(`${count} arquivo${count === 1 ? '' : 's'} ${explorerClipboard.mode === 'copy' ? 'copiado' : 'movido'}${count === 1 ? '' : 's'} para esta pasta.`);
            if (explorerClipboard.mode === 'cut') setExplorerClipboard(null);
            clearSelection();
        } catch (error) {
            toast.error(`Não foi possível colar: ${String(error)}`);
        }
    }, [explorerClipboard, activeFolderId, rootView, queryClient, clearSelection]);

    useKeyboardShortcuts({
        onSelectAll: handleSelectAll,
        onDelete: handleKeyboardDelete,
        onEscape: handleEscape,
        onSearch: handleFocusSearch,
        onEnter: handleEnter,
        onDownload: handleKeyboardDownload,
        onShare: handleKeyboardShare,
        onRename: handleKeyboardRename,
        onCreateFolder: handleKeyboardCreateFolder,
        onGoUp: handleKeyboardGoUp,
        onCopy: handleExplorerCopy,
        onCut: handleExplorerCut,
        onPaste: handleExplorerPaste,
        enabled: !previewFile && !playingFile && !pdfFile && !archiveViewFile && !showMoveModal
    });

    const handlePreview = (file: TelegramFile, orderedFiles?: TelegramFile[]) => {
        const contextFiles = (orderedFiles || displayedFiles).filter((f) => f.type !== 'folder');

        // 14.21L: sempre resolver o objeto mais atual pelo ID.
        const currentFile =
            contextFiles.find((f) => f.id === file.id) ??
            displayedFiles.find((f) => f.id === file.id) ??
            file;

        const contextIndex = contextFiles.findIndex((f) => f.id === currentFile.id);

        setPreviewContextFiles(contextFiles);
        setPreviewContextIndex(contextIndex);

        const currentName = (currentFile.name ?? '').trim();
        const mediaExt = (currentName.split('.').pop() ?? '').trim().toLowerCase();

        const isMedia =
            isMediaFile(currentName) ||
            ['mp4','webm','mkv','mov','avi','ogg','mp3','wav','aac','flac','m4a','opus'].includes(mediaExt);

        const isPdf = isPdfFile(currentName);
        const isArchive = isArchiveFile(currentName);

        if (isArchive) {
            setArchiveViewFile(currentFile);
            setPreviewFile(null);
            setPlayingFile(null);
            setPdfFile(null);
        } else if (isMedia) {
            setPlayingFile(currentFile);
            setPreviewFile(null);
            setPdfFile(null);
            setArchiveViewFile(null);
        } else if (isPdf) {
            setPdfFile(currentFile);
            setPreviewFile(null);
            setPlayingFile(null);
            setArchiveViewFile(null);
        } else {
            setPreviewFile(currentFile);
            setPlayingFile(null);
            setPdfFile(null);
            setArchiveViewFile(null);
        }
    };

    useEffect(() => {
        const cleanups: Array<() => void> = [];
        void listen<string>('tcloud-tray-action', (event) => {
            if (event.payload === 'upload') void handleManualUpload();
            if (event.payload === 'status') {
                setAdvancedInitialTab('health');
                setShowAdvancedCenter(true);
            }
            if (event.payload === 'pause-sync' || event.payload === 'sync') {
                setShowTCloudSyncCenter(true);
            }
            if (event.payload === 'settings') {
                setShowSettings(true);
            }
        }).then((unlisten) => cleanups.push(unlisten));
        const previewHandler = (event: Event) => {
            const detail = (event as CustomEvent<{id:number;contextIds?:number[]}>).detail;
            const id = Number(detail?.id);
            const file = allFiles.find((item) => item.id === id);
            if (!file) return;
            const explicit = Array.isArray(detail?.contextIds) ? detail.contextIds : [];
            const contextFiles = explicit.length > 0
                ? explicit.map((candidateId) => allFiles.find((item) => item.id === candidateId)).filter((item): item is TelegramFile => Boolean(item))
                : allFiles;
            handlePreview(file, contextFiles.length > 0 ? contextFiles : allFiles);
        };
        window.addEventListener('tcloud-open-preview-id', previewHandler);
        return () => { cleanups.forEach((fn)=>fn()); window.removeEventListener('tcloud-open-preview-id', previewHandler); };
    }, [allFiles, handleManualUpload, handlePreview]);


    const navigatePreview = useCallback((step: 1 | -1) => {
        if (previewContextFiles.length === 0) return;

        const currentFileId = previewFile?.id ?? playingFile?.id ?? pdfFile?.id ?? archiveViewFile?.id;
        if (!currentFileId) return;

        const currentIndex = previewContextFiles.findIndex((f) => f.id === currentFileId);
        if (currentIndex === -1) return;

        const nextIndex = (currentIndex + step + previewContextFiles.length) % previewContextFiles.length;
        const nextFile = previewContextFiles[nextIndex];
        if (!nextFile) return;

        setPreviewContextIndex(nextIndex);

        const mediaExt = (nextFile.name.split('.').pop() ?? '').trim().toLowerCase();

        const isMedia =
            isMediaFile(nextFile.name) ||
            ['mp4','webm','mkv','mov','avi','ogg','mp3','wav','aac','flac','m4a','opus'].includes(mediaExt);
        const isPdf = isPdfFile(nextFile.name);
        const isArchive = isArchiveFile(nextFile.name);

        if (isArchive) {
            setArchiveViewFile(nextFile);
            setPreviewFile(null);
            setPlayingFile(null);
            setPdfFile(null);
        } else if (isMedia) {
            setPlayingFile(nextFile);
            setPreviewFile(null);
            setPdfFile(null);
            setArchiveViewFile(null);
        } else if (isPdf) {
            setPdfFile(nextFile);
            setPreviewFile(null);
            setPlayingFile(null);
            setArchiveViewFile(null);
        } else {
            setPreviewFile(nextFile);
            setPlayingFile(null);
            setPdfFile(null);
            setArchiveViewFile(null);
        }
    }, [previewContextFiles, previewFile, playingFile, pdfFile, archiveViewFile]);

    const handleNextPreview = useCallback(() => {
        navigatePreview(1);
    }, [navigatePreview]);

    const handlePrevPreview = useCallback(() => {
        navigatePreview(-1);
    }, [navigatePreview]);

    const previewNeighborFiles = useCallback(() => {
        if (previewContextFiles.length === 0) {
            return { nextFile: null as TelegramFile | null, prevFile: null as TelegramFile | null };
        }

        const currentFileId = previewFile?.id ?? playingFile?.id ?? pdfFile?.id ?? archiveViewFile?.id;
        if (!currentFileId) {
            return { nextFile: null as TelegramFile | null, prevFile: null as TelegramFile | null };
        }

        const currentIdx = previewContextFiles.findIndex((f) => f.id === currentFileId);
        if (currentIdx === -1) {
            return { nextFile: null as TelegramFile | null, prevFile: null as TelegramFile | null };
        }

        const nextIdx = (currentIdx + 1) % previewContextFiles.length;
        const prevIdx = (currentIdx - 1 + previewContextFiles.length) % previewContextFiles.length;

        return {
            nextFile: previewContextFiles[nextIdx] || null,
            prevFile: previewContextFiles[prevIdx] || null,
        };
    }, [previewContextFiles, previewFile, playingFile, pdfFile, archiveViewFile]);

    const handleMoveFilesToFolder = async (idsToMove: number[], targetFolderId: number | null) => {
        if (idsToMove.length === 0) return;
        if (activeFolderId === targetFolderId) {
            toast.info('File is already in this folder');
            return;
        }

        if (idsToMove.length >= 10) {
            const confirmed = await confirm({
                title: 'Bulk Move Confirmation',
                message: `You are about to move ${idsToMove.length} files. Are you sure?`,
                confirmText: `Move ${idsToMove.length} Files`,
                variant: 'info',
            });
            if (!confirmed) return;
        }

        try {
            await invoke('cmd_move_files', {
                messageIds: idsToMove,
                sourceFolderId: activeFolderId,
                targetFolderId: targetFolderId
            });
            // Clean up stale thumbnail and preview cache entries for the old message IDs
            await Promise.all(idsToMove.flatMap(id => [
                invoke('cmd_delete_image_thumbnail', { messageId: id, folderId: activeFolderId }).catch(() => {}),
                invoke('cmd_delete_preview_for_message', { messageId: id, folderId: activeFolderId }).catch(() => {}),
            ]));

            queryClient.invalidateQueries({ queryKey: ['files', activeFolderId] });
            setSelectedIds([]);
            toast.success(`Moved ${idsToMove.length} file(s).`);
        } catch {
            toast.error(`Failed to move file(s).`);
        }
    };

    const handleInternalDragStart = (event: DragStartEvent) => {
        setInternalDragTarget(null);
        if (event.active.data.current?.kind !== 'telegram-files') return;
        const fileIds = event.active.data.current.fileIds;
        if (!Array.isArray(fileIds) || fileIds.length === 0) return;
        setInternalDrag({
            fileIds: fileIds.filter((id): id is number => typeof id === 'number'),
            label: String(event.active.data.current.label || ''),
        });
    };

    const handleInternalDragOver = (event: DragOverEvent) => {
        if (event.active.data.current?.kind !== 'telegram-files') {
            setInternalDragTarget(null);
            return;
        }
        const overKind = event.over?.data.current?.kind;
        if (overKind !== 'sidebar-folder' && overKind !== 'content-folder') {
            setInternalDragTarget(null);
            return;
        }
        const folderId = event.over?.data.current?.folderId;
        if (folderId === null) {
            setInternalDragTarget('Mensagens Salvas');
            return;
        }
        if (typeof folderId === 'number') {
            setInternalDragTarget(folders.find((folder) => folder.id === folderId)?.name || 'Pasta');
            return;
        }
        setInternalDragTarget(null);
    };

    const handleInternalDragEnd = async (event: DragEndEvent) => {
        const { active, over } = event;
        setInternalDrag(null);
        setInternalDragTarget(null);
        if (!over) return;

        const activeKind = active.data.current?.kind;
        const overKind = over.data.current?.kind;

        if (activeKind === 'telegram-files') {
            const fileIds = active.data.current?.fileIds;
            const targetFolderId = over.data.current?.folderId;
            const isFolderTarget = overKind === 'sidebar-folder' || overKind === 'content-folder';
            if (isFolderTarget && Array.isArray(fileIds) && (targetFolderId === null || typeof targetFolderId === 'number')) {
                await handleMoveFilesToFolder(
                    fileIds.filter((id): id is number => typeof id === 'number'),
                    targetFolderId,
                );
            }
            return;
        }

        if (activeKind === 'sidebar-folder') {
            const draggedFolderId = active.data.current?.folderId;
            if (typeof draggedFolderId !== 'number') return;

            if (overKind === 'sidebar-group') {
                const groupId = over.data.current?.groupId;
                await handleAssignFolderToGroup(draggedFolderId, typeof groupId === 'number' ? groupId : null);
                return;
            }

            if (overKind === 'sidebar-folder') {
                const overFolderId = over.data.current?.folderId;
                if (typeof overFolderId !== 'number' || draggedFolderId === overFolderId) return;
                const oldIndex = folders.findIndex(folder => folder.id === draggedFolderId);
                const newIndex = folders.findIndex(folder => folder.id === overFolderId);
                if (oldIndex !== -1 && newIndex !== -1) {
                    await handleReorderFolders(arrayMove(folders, oldIndex, newIndex));
                }
            }
            return;
        }

        if (activeKind === 'sidebar-group' && overKind === 'sidebar-group') {
            const draggedGroupId = active.data.current?.groupId;
            const overGroupId = over.data.current?.groupId;
            if (typeof draggedGroupId !== 'number' || typeof overGroupId !== 'number' || draggedGroupId === overGroupId) return;
            const oldIndex = groups.findIndex(group => group.id === draggedGroupId);
            const newIndex = groups.findIndex(group => group.id === overGroupId);
            if (oldIndex !== -1 && newIndex !== -1) {
                await handleUpdateGroupOrder(arrayMove(groups, oldIndex, newIndex));
            }
        }
    };

    const activeForumParent = activeFolderId === null
        ? null
        : forumTree.find((forum) => forum.topics.some((topic) => topic.id === activeFolderId)) ?? null;
    const activeForumTopic = activeForumParent?.topics.find((topic) => topic.id === activeFolderId) ?? null;

    const breadcrumbs = useMemo<ExplorerBreadcrumbItem[]>(() => {
        const root: ExplorerBreadcrumbItem = {
            label: 'TCloud',
            onClick: () => { setActiveFolderId(null); setSelectedForum(null); setRootView('dashboard'); },
        };
        if (selectedForum && activeFolderId === null) {
            return selectedForum.name.trim().toLocaleLowerCase('pt-BR') === 'tcloud'
                ? [root]
                : [root, { label: selectedForum.name }];
        }
        if (activeForumParent && activeForumTopic) {
            const parentIsRootName = activeForumParent.name.trim().toLocaleLowerCase('pt-BR') === 'tcloud';
            return [
                root,
                ...(!parentIsRootName ? [{
                    label: activeForumParent.name,
                    onClick: () => { setActiveFolderId(null); setRootView('dashboard'); setSelectedForum(activeForumParent); },
                }] : []),
                { label: activeForumTopic.name },
            ];
        }
        if (activeFolderId !== null) {
            return [root, { label: folders.find((folder) => folder.id === activeFolderId)?.name || 'Pasta' }];
        }
        const special = rootView === 'saved' ? 'Mensagens Salvas'
            : rootView === 'recent' ? 'Recentes'
            : rootView === 'favorites' ? 'Favoritos'
            : rootView === 'problems' ? 'Precisa de atenção'
            : rootView === 'sync' ? 'Sincronização'
            : rootView === 'trash' ? 'Lixeira'
            : rootView === 'gallery' ? 'Galeria'
            : rootView === 'music' ? 'Música'
            : rootView === 'offline' ? 'Neste computador'
            : rootView === 'vault' ? 'Cofre'
            : null;
        return special ? [root, { label: special }] : [root];
    }, [selectedForum, activeFolderId, activeForumParent, activeForumTopic, rootView, folders, setActiveFolderId]);

    const handleNavigateUp = useCallback(() => {
        if (activeFolderId !== null) {
            if (activeForumParent) {
                setActiveFolderId(null);
                setRootView('dashboard');
                setSelectedForum(activeForumParent);
                return;
            }

            setActiveFolderId(null);
            setSelectedForum(null);
            setRootView('dashboard');
            return;
        }

        if (selectedForum) {
            setSelectedForum(null);
            setRootView('dashboard');
            return;
        }

        if (rootView !== 'dashboard') {
            setSelectedForum(null);
            setActiveFolderId(null);
            setRootView('dashboard');
        }
    }, [
        activeFolderId,
        activeForumParent,
        selectedForum,
        rootView,
        setActiveFolderId,
    ]);

    const canNavigateUp =
        activeFolderId !== null ||
        selectedForum !== null ||
        rootView !== 'dashboard';
    const currentFolderName = selectedForum && activeFolderId === null
        ? selectedForum.name
        : activeFolderId === null
            ? rootView === 'saved' ? t('common.saved_messages')
            : rootView === 'recent' ? 'Recentes'
            : rootView === 'favorites' ? 'Favoritos'
            : rootView === 'problems' ? 'Precisa de atenção'
            : rootView === 'sync' ? 'Sincronização'
            : rootView === 'trash' ? 'Lixeira'
            : rootView === 'gallery' ? 'Galeria'
            : rootView === 'music' ? 'Música'
            : rootView === 'offline' ? 'Neste computador'
            : rootView === 'vault' ? 'Cofre'
            : 'Início'
            : activeForumTopic?.name || folders.find(f => f.id === activeFolderId)?.name || t('common.folders');


    const computerConnected = Boolean(explorerConnection?.provider_connected);
    const homeSyncCount = Math.max(syncingCount, isSyncing ? 1 : 0);

    const selectedStatusSize = useMemo(() => {
        if (selectedIds.length === 0) return 0;

        return displayedFiles.reduce((total, file) => {
            if (!selectedIdSet.has(file.id) || file.type === 'folder') {
                return total;
            }

            const size = Number(file.size);

            return total + (
                Number.isFinite(size) && size > 0
                    ? size
                    : 0
            );
        }, 0);
    }, [displayedFiles, selectedIdSet, selectedIds.length]);

    const homeHealth = !isConnected
        ? { tone: 'offline', title: 'Sem conexÃ£o com a nuvem', detail: 'O TCloud vai continuar tentando quando a internet voltar.' }
        : explorerConnection === undefined
            ? { tone: 'busy', title: 'Verificando este computador', detail: 'Confirmando a conexÃ£o com o Explorador do Windows.' }
            : !computerConnected
                ? { tone: 'busy', title: 'Conectando ao Explorer', detail: 'A reconexão é automática. Você pode continuar usando o TCloud.' }
                : problemCount > 0
                    ? { tone: 'danger', title: `${problemCount} ${problemCount === 1 ? 'item precisa' : 'itens precisam'} de atenção`, detail: 'O TCloud já tentou resolver sozinho. Abra Problemas para revisar.' }
                    : homeSyncCount > 0
                        ? { tone: 'busy', title: 'Sincronizando seus arquivos', detail: `${homeSyncCount} ${homeSyncCount === 1 ? 'item está' : 'itens estão'} sendo atualizado${homeSyncCount === 1 ? '' : 's'} em segundo plano.` }
                        : { tone: 'ok', title: 'Tudo atualizado', detail: 'Seus arquivos estÃ£o prontos no TCloud e no Explorer.' };

    const previewNeighbors = previewNeighborFiles();

    const handleRunBackup = useCallback(async (backup: ProBackupSource) => {
        const paths = await listBackupFiles(backup.id);
        let success = 0;
        for (const path of paths) {
            try {
                await invoke('cmd_upload_file', {
                    path,
                    folderId: backup.target_folder_id,
                    transferId: `backup-manual-${backup.id}-${Date.now()}-${success}`,
                    protectionMode: settings.encryptionDefaultMode === 'vault' ? 'vault' : 'standard',
                    promptToken: null,
                    protectMetadata: false,
                });
                await markBackupFile(backup.id, path);
                success += 1;
            } catch (error) {
                console.warn('Manual backup upload failed', path, error);
            }
        }
        await markBackupRun(backup.id);
        queryClient.invalidateQueries({ queryKey: ['files', backup.target_folder_id] });
        toast.success(`Backup concluído: ${success}/${paths.length} arquivos enviados.`);
    }, [queryClient, settings.encryptionDefaultMode]);

    const handleBulkAvailability = useCallback(async (offline: boolean) => {
        const selectedFiles = displayedFiles.filter((file) => selectedIdSet.has(file.id) && file.type !== 'folder');
        if (selectedFiles.length === 0) return;
        const results = await Promise.allSettled(selectedFiles.map((file) =>
            setCfapiItemAvailability(file.id, file.folder_id ?? activeFolderId, offline)
        ));
        const successIds = new Set(selectedFiles.filter((_, index) => results[index].status === 'fulfilled').map((file) => file.id));
        queryClient.setQueryData<TelegramFile[]>(['files', activeFolderId], (current = []) => current.map((item) =>
            successIds.has(item.id) ? { ...item, local_status: offline ? 'cached' : 'cloud_only', is_pinned: offline } : item
        ));
        const failed = results.length - successIds.size;
        if (failed === 0) {
            toast.success(offline ? `${successIds.size} arquivo(s) disponível(is) offline.` : `Espaço liberado em ${successIds.size} arquivo(s).`);
        } else {
            toast.warning(`${successIds.size} concluído(s) e ${failed} com problema.`);
        }
    }, [activeFolderId, displayedFiles, queryClient, selectedIds]);

    const handleRetryAllProblems = useCallback(async () => {
        const tasks: Promise<unknown>[] = [retryAllTCloudSyncFailures()];
        if ((explorerConnection?.failed_operations ?? 0) > 0) tasks.push(retryFailedCfapiOperations());
        const results = await Promise.allSettled(tasks);
        const failed = results.filter((result) => result.status === 'rejected').length;
        await Promise.all([
            queryClient.invalidateQueries({ queryKey: ['tcloud-sync-overview'] }),
            queryClient.invalidateQueries({ queryKey: ['cfapi-status'] }),
            queryClient.invalidateQueries({ queryKey: ['files'] }),
        ]);
        if (failed === 0) toast.success('O TCloud colocou todos os problemas atuais novamente na fila.');
        else toast.warning('Algumas ações ainda precisam de atenção.');
    }, [explorerConnection?.failed_operations, queryClient]);

    const handleCleanupHistoricalProblems = useCallback(async () => {
        try {
            const result = await cleanupStaleTCloudAttention();
            await Promise.all([
                queryClient.invalidateQueries({ queryKey: ['tcloud-sync-overview'] }),
                queryClient.invalidateQueries({ queryKey: ['tcloud-sync-center'] }),
            ]);
            const cleaned = result.cleaned_sync_jobs + result.cleaned_cfapi_ops;
            if (cleaned > 0) toast.success(`${cleaned} registro(s) antigo(s) de sincronização foram removidos.`);
            else toast.info('Não há registros antigos para limpar.');
        } catch (error) {
            toast.error(`Não foi possível limpar o histórico: ${String(error)}`);
        }
    }, [queryClient]);

    const handleMainWheel = useCallback((event: React.WheelEvent<HTMLElement>) => {
        if (event.ctrlKey && event.deltaY !== 0) {
            event.preventDefault();
            setCardScale((current) => Math.max(0.5, Math.min(2, current + (event.deltaY < 0 ? 0.125 : -0.125))));
            return;
        }
        if (event.deltaY === 0) return;

        const target = event.target as HTMLElement;
        // Dedicated scroll areas (file explorer, modals, queues) handle their
        // own wheel events. This fallback is mainly for the dashboard header
        // and empty space above the file grid.
        if (target.closest('.tcloud-explorer, .tcloud-pro-content, [data-wheel-scroll="self"], input, textarea, select')) {
            return;
        }

        const explorer = event.currentTarget.querySelector<HTMLElement>('.tcloud-explorer');
        if (!explorer || explorer.scrollHeight <= explorer.clientHeight + 1) return;

        const multiplier = event.deltaMode === 1
            ? 32
            : event.deltaMode === 2
                ? explorer.clientHeight
                : 1;
        event.preventDefault();
        explorer.scrollTop += event.deltaY * multiplier;
    }, []);

    return (
        <DndContext
            sensors={dragSensors}
            collisionDetection={closestCenter}
            onDragStart={handleInternalDragStart}
            onDragOver={handleInternalDragOver}
            onDragCancel={() => { setInternalDrag(null); setInternalDragTarget(null); }}
            onDragEnd={handleInternalDragEnd}
        >
            <div className="desktop-shell tcloud-desktop-shell relative flex h-screen w-full overflow-hidden bg-app-canvas">

            <ExternalDropBlocker
                currentFolderName={currentFolderName}
                enabled={isConnected}
                onFilesDropped={handleDropFromCurrentView}
                onUploadClick={handleUploadFromCurrentView}
            />

            <Suspense fallback={<LazyOverlayFallback />}>
                {showMoveModal && (
                    <MoveToFolderModal
                        folders={folders}
                        fileName={moveFileTarget?.name}
                        onClose={() => { setShowMoveModal(false); setMoveFileTarget(null); }}
                        onSelect={async (targetFolderId: number | null) => {
                            if (moveFileTarget) {
                                try {
                                    await invoke('cmd_move_files', {
                                        messageIds: [moveFileTarget.id],
                                        sourceFolderId: activeFolderId,
                                        targetFolderId,
                                    });
                                    // Clean up stale thumbnail and preview cache for the old message ID
                                    await Promise.all([
                                        invoke('cmd_delete_image_thumbnail', { messageId: moveFileTarget.id, folderId: activeFolderId }).catch(() => {}),
                                        invoke('cmd_delete_preview_for_message', { messageId: moveFileTarget.id, folderId: activeFolderId }).catch(() => {}),
                                    ]);
                                    queryClient.invalidateQueries({ queryKey: ['files', activeFolderId] });
                                    toast.success(`Moved "${moveFileTarget.name}"`);
                                    setMoveFileTarget(null);
                                    setShowMoveModal(false);
                                } catch {
                                    toast.error('Failed to move file');
                                }
                            } else {
                                handleBulkMove(targetFolderId, () => setShowMoveModal(false));
                            }
                        }}
                        activeFolderId={activeFolderId}
                        key="move-modal"
                    />
                )}
                {playingFile && (
                    <MediaPlayer
                        file={playingFile}
                        onClose={() => setPlayingFile(null)}
                        onNext={handleNextPreview}
                        onPrev={handlePrevPreview}
                        currentIndex={previewContextIndex}
                        totalItems={previewContextFiles.length}
                        activeFolderId={activeFolderId}
                        key={playingFile.id}
                    />
                )}
                {pdfFile && (
                    <PdfViewer
                        file={pdfFile}
                        onClose={() => setPdfFile(null)}
                        onNext={handleNextPreview}
                        onPrev={handlePrevPreview}
                        currentIndex={previewContextIndex}
                        totalItems={previewContextFiles.length}
                        activeFolderId={activeFolderId}
                        key="pdf-viewer"
                    />
                )}
                {showRemoteUpload && (
                    <RemoteUploadModal
                        isOpen={showRemoteUpload}
                        onClose={() => setShowRemoteUpload(false)}
                        folders={folders}
                        onUpload={handleUrlUpload}
                        key="remote-upload-modal"
                    />
                )}
            </Suspense>

            <Sidebar
                folders={folders}
                groups={groups}
                activeFolderId={activeFolderId}
                activeForumId={selectedForum?.id ?? activeForumParent?.id ?? null}
                setActiveFolderId={(id) => {
                    setActiveFolderId(id);
                    if (id !== null) {
                        setSelectedForum(null);
                        setRootView('dashboard');
                    }
                }}
                onWarmFolder={warmFolderForNavigation}
                rootView={rootView}
                onRootViewChange={(view) => {
                    setSelectedForum(null);
                    setRootView(view);
                }}
                onDelete={handleFolderDelete}
                onRename={async (id, name) => {
                    await handleFolderRename(id, name);

                    await Promise.all([
                        queryClient.invalidateQueries({ queryKey: ['folders'] }),
                        queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] }),
                    ]);

                    await queryClient.refetchQueries({
                        queryKey: ['tcloud-forum-tree']
                    });
                }}
                onToggleVisibility={async (id, _name, isPublic) => {
                    try {
                        await handleFolderToggleVisibility(id, !isPublic);
                        queryClient.invalidateQueries({ queryKey: ['folders'] });
                    } catch { /* toast handled in hook */ }
                }}
                onExportInvite={async (id, _name) => {
                    try {
                        const info = await handleExportFolderInvite(id);
                        try {
                            await copyToClipboard(info.link);
                            toast.success(`Invite link copied: ${info.link}`);
                        } catch (e) {
                            toast.error(`Failed to copy to clipboard: ${e}`);
                        }
                    } catch { /* backend error already toasted in hook */ }
                }}
                onCreate={createFolderInCurrentContext}
                isConnected={isConnected}
                onLogout={handleLogout}
                bandwidth={bandwidth || null}
                onAssignFolderToGroup={handleAssignFolderToGroup}
                onCreateGroup={handleCreateGroup}
                onUpdateGroup={handleUpdateGroup}
                onDeleteGroup={handleDeleteGroup}
                onOpenSettings={() => setShowSettings(true)}
                onOpenForums={() => setShowForumManager(true)}
                onOpenAdvanced={() => { setAdvancedInitialTab('index'); setShowAdvancedCenter(true); }}
                problemCount={problemCount}
                syncingCount={syncingCount}
                onOpenForum={(forum) => {
                    setActiveFolderId(null);
                    setRootView('dashboard');
                    setSelectedForum(forum);
                }}
            />

            <main className="tcloud-main-pane flex min-h-0 min-w-0 flex-1 flex-col" onWheelCapture={handleMainWheel}>
                <TopBar
                    currentFolderName={currentFolderName}
                    breadcrumbs={breadcrumbs}
                    selectedIds={selectedIds}
                    onShowMoveModal={() => setShowMoveModal(true)}
                    onBulkDownload={handleBulkDownload}
                    onBulkDelete={handleBulkDelete}
                    onBulkShare={handleBulkShare}
                    onBulkKeepOffline={() => void handleBulkAvailability(true)}
                    onBulkFreeSpace={() => void handleBulkAvailability(false)}
                    onDownloadFolder={handleDownloadFolder}
                    onClearSelection={clearSelection}
                    onDetailsClick={() => setShowDetailsPanel(true)}
                    onRenameSelection={handleKeyboardRename}
                    onUploadClick={handleUploadFromCurrentView}
                    onFolderUploadClick={handleFolderUploadFromCurrentView}
                    onCreateFolderRequest={beginCreateFolder}
                    onCopySelection={handleExplorerCopy}
                    onCutSelection={handleExplorerCut}
                    onPaste={() => void handleExplorerPaste()}
                    clipboardCount={explorerClipboard?.items.length ?? 0}
                    onNavigateRoot={() => { setActiveFolderId(null); setSelectedForum(null); setRootView('dashboard'); }}
                    onBack={handleNavigationBack}
                    onForward={handleNavigationForward}

                    onUp={handleNavigateUp}
                    canGoBack={navigationIndex > 0}
                    canGoForward={navigationIndex < navigationHistoryRef.current.length - 1}

                    canGoUp={canNavigateUp}
                    isConnected={isConnected}
                    isSyncing={isSyncing}
                    onSync={handleSyncFolders}
                    viewMode={viewMode}
                    setViewMode={setViewMode}
                    cardScale={cardScale}
                    onCardScaleChange={setCardScale}
                    sortField={sortField}
                    sortDirection={sortDirection}
                    onSortChange={handleSortChange}
                    searchTerm={searchTerm}
                    onSearchChange={setSearchTerm}
                    fileFilter={fileFilter}
                    dateFilter={dateFilter}
                    activeFilterCount={activeFilterCount}
                    onFileFilterChange={setFileFilter}
                    onDateFilterChange={setDateFilter}
                    onClearFilters={() => {
                        setFileFilter('all');
                        setDateFilter('any');
                    }}
                    onSettingsClick={() => setShowSettings(true)}
                    onTCloudClick={() => setShowTCloudDashboard(true)}
                    onTCloudSyncClick={() => { setSelectedForum(null); setActiveFolderId(null); setRootView('sync'); }}
                    problemCount={problemCount}
                    syncingCount={syncingCount}
                    onRemoteUploadClick={() => setShowRemoteUpload(true)}
                    onOpenExplorer={async () => { await openCfapiRoot(); }}
                    canCreateItems={activeFolderId !== null || rootView === 'saved' || rootView === 'dashboard'}
                    canUploadItems={activeFolderId !== null || rootView === 'saved'}
                    showFileTools={!showUtilityWorkspace && !showHomeDashboard}
                />

                {inlineEditor && (
                    <ExplorerInlineEditor
                        mode={inlineEditor.mode}
                        value={inlineEditor.value}
                        busy={inlineEditorBusy}
                        locationLabel={currentFolderName}
                        onChange={(value) => setInlineEditor((current) => current ? { ...current, value } as InlineExplorerEditor : current)}
                        onSubmit={submitInlineEditor}
                        onCancel={() => setInlineEditor(null)}
                    />
                )}

                {showForumOverview && selectedForum && (
                    <ForumOverview
                        forum={selectedForum}
                        onOpenFolder={(folderId) => {
                            setSelectedForum(null);
                            setActiveFolderId(folderId);
                        }}
                        onWarmFolder={warmFolderForNavigation}
                        onCreateFolder={beginCreateFolder}
                    />
                )}

                {showHomeDashboard && (
                    <section className="tcloud-drive-home">
                        <div className="tcloud-drive-home-header tcloud-home-compact-header">
                            <div>
                                <h2>Início</h2>
                                <p>Seus arquivos, pastas e itens importantes em um só lugar.</p>
                            </div>
                        </div>

                        {homeHealth.tone !== 'ok' && (
                            <section className={`tcloud-home-health is-${homeHealth.tone}`}>
                                <div className="tcloud-home-health-icon">
                                    {homeHealth.tone === 'danger' ? <CircleAlert className="h-4 w-4" /> : homeHealth.tone === 'offline' ? <CloudOff className="h-4 w-4" /> : <RefreshCw className="h-4 w-4 animate-spin" />}
                                </div>
                                <div className="tcloud-home-health-copy">
                                    <strong>{homeHealth.title}</strong>
                                    <span>{homeHealth.detail}</span>
                                </div>
                                <div className="tcloud-home-health-actions">
                                    {problemCount > 0 ? (
                                        <button type="button" className="is-primary" onClick={() => { setActiveFolderId(null); setRootView('problems'); }}>Revisar</button>
                                    ) : !computerConnected ? (
                                        <button type="button" className="is-primary" onClick={() => setShowSettings(true)}>Configurar</button>
                                    ) : homeSyncCount > 0 ? (
                                        <button type="button" onClick={() => { setSelectedForum(null); setActiveFolderId(null); setRootView('sync'); }}>Ver atividade</button>
                                    ) : null}
                                </div>
                            </section>
                        )}

                        <div className="tcloud-home-locations" aria-label="Acesso rápido">
                            <button type="button" onClick={() => { setSelectedForum(null); setActiveFolderId(null); setRootView('saved'); }}>
                                <HardDrive className="h-4 w-4" />
                                <span><strong>Meu TCloud</strong><small>Arquivos da sua nuvem</small></span>
                            </button>
                            <button type="button" onClick={() => { setSelectedForum(null); setActiveFolderId(null); setRootView('favorites'); }}>
                                <Star className="h-4 w-4" />
                                <span><strong>Favoritos</strong><small>{dashboardFavoriteFiles.length} itens marcados</small></span>
                            </button>
                            <button type="button" onClick={() => void openCfapiRoot()}>
                                <FolderOpen className="h-4 w-4" />
                                <span><strong>TCloud no Windows</strong><small>{computerConnected ? 'Abrir no Explorador' : 'Conectar este computador'}</small></span>
                            </button>
                            <button type="button" onClick={() => { setSelectedForum(null); setActiveFolderId(null); setRootView('vault'); }}>
                                <LockKeyhole className="h-4 w-4" />
                                <span><strong>Cofre</strong><small>Arquivos protegidos</small></span>
                            </button>
                        </div>

                        <div className="tcloud-quick-access">
                            <div className="tcloud-section-heading tcloud-section-heading-compact">
                                <div>
                                    <h3>Recentes</h3>
                                    <span>Continue de onde parou</span>
                                </div>
                            </div>

                            <div className="tcloud-quick-grid">
                                {dashboardRecentFiles.slice(0, 4).map((file) => (
                                    <button
                                        type="button"
                                        key={file.id}
                                        className="tcloud-quick-card"
                                        onClick={() => handlePreview(file, dashboardRecentFiles)}
                                    >
                                        <div className="tcloud-quick-card-icon">
                                            {/\.(jpg|jpeg|png|gif|webp|bmp|heic|avif)$/i.test(file.name) ? <Image className="h-5 w-5" /> :
                                             /\.(mp4|mkv|mov|avi|webm|m4v|mpeg|mpg)$/i.test(file.name) ? <Video className="h-5 w-5" /> :
                                             /\.(mp3|wav|m4a|aac|flac|ogg|opus|wma)$/i.test(file.name) ? <Music className="h-5 w-5" /> :
                                             <FileText className="h-5 w-5" />}
                                        </div>
                                        <div className="min-w-0 text-left">
                                            <strong>{file.name}</strong>
                                            <span>{file.sizeStr}</span>
                                        </div>
                                    </button>
                                ))}
                            </div>
                        </div>

                        {dashboardFavoriteFiles.length > 0 && (
                            <div className="tcloud-favorite-strip">
                                <div className="tcloud-section-heading tcloud-section-heading-compact">
                                    <div>
                                        <h3>Favoritos</h3>
                                        <span>Arquivos importantes</span>
                                    </div>
                                    <Star className="h-3.5 w-3.5 text-app-warning" />
                                </div>
                                <div className="tcloud-favorite-strip-list">
                                    {dashboardFavoriteFiles.slice(0, 5).map((file) => (
                                        <button
                                            type="button"
                                            key={file.id}
                                            onClick={() => handlePreview(file, dashboardFavoriteFiles)}
                                        >
                                            <Star className="h-3.5 w-3.5 fill-current text-app-warning" />
                                            <span>{file.name}</span>
                                        </button>
                                    ))}
                                </div>
                            </div>
                        )}

                        <div className="tcloud-all-files-section">
                            <div className="tcloud-section-heading tcloud-section-heading-files">
                                <div>
                                    <h3>Meus arquivos</h3>
                                    <span>Itens da sua nuvem</span>
                                </div>
                                <div className="tcloud-home-file-meta">
                                    <button type="button" onClick={() => setRootView('saved')}>
                                        Abrir Meu TCloud
                                        <ArrowRight className="h-3.5 w-3.5" />
                                    </button>
                                </div>
                            </div>

                            <div className="tcloud-home-file-grid" role="list" aria-label="Meus arquivos recentes">
                                {dashboardMainFiles.map((file) => {
                                    const isLocal = ['cached', 'offline', 'local', 'available'].some((value) => (file.local_status || '').toLowerCase().includes(value));
                                    const isImage = /\.(jpg|jpeg|png|gif|webp|bmp|heic|avif)$/i.test(file.name);
                                    const isVideo = /\.(mp4|mkv|mov|avi|webm|m4v|mpeg|mpg)$/i.test(file.name);
                                    const isAudio = /\.(mp3|wav|m4a|aac|flac|ogg|opus|wma)$/i.test(file.name);
                                    return (
                                        <button
                                            type="button"
                                            role="listitem"
                                            key={`${file.folder_id ?? 'saved'}:${file.id}`}
                                            className="tcloud-home-file-tile"
                                            onClick={() => handlePreview(file, dashboardMainFiles)}
                                            title={file.name}
                                        >
                                            <span className="tcloud-home-file-icon">
                                                {isImage ? <Image className="h-4 w-4" /> : isVideo ? <Video className="h-4 w-4" /> : isAudio ? <Music className="h-4 w-4" /> : <FileText className="h-4 w-4" />}
                                            </span>
                                            <span className="tcloud-home-file-copy">
                                                <strong>{file.name}</strong>
                                                <small>{file.sizeStr}</small>
                                            </span>
                                            <span className={`tcloud-home-file-state ${isLocal ? 'is-local' : 'is-cloud'}`} title={isLocal ? 'Disponível neste computador' : 'Na nuvem'}>
                                                {isLocal ? <CheckCircle2 className="h-3.5 w-3.5" /> : <Cloud className="h-3.5 w-3.5" />}
                                            </span>
                                        </button>
                                    );
                                })}
                                {dashboardMainFiles.length === 0 && <div className="tcloud-home-file-empty">Nenhum arquivo para mostrar.</div>}
                            </div>
                        </div>
                    </section>
                )}

                {activeFolderId === null && rootView === 'gallery' && !hasActiveSearch && activeFilterCount === 0 && (
                    <section className="tcloud-collection-heading"><div className="tcloud-workspace-icon is-accent"><Image className="h-5 w-5" /></div><div><h2>Galeria</h2><p>Fotos e vídeos organizados no mesmo espaço de navegação dos arquivos.</p></div><span>{displayedFiles.length} item(ns)</span></section>
                )}
                {activeFolderId === null && rootView === 'music' && !hasActiveSearch && activeFilterCount === 0 && (
                    <section className="tcloud-collection-heading"><div className="tcloud-workspace-icon is-accent"><Music className="h-5 w-5" /></div><div><h2>Música</h2><p>Arquivos de áudio prontos para abrir no player do TCloud.</p></div><span>{displayedFiles.length} faixa(s)</span></section>
                )}
                {activeFolderId === null && rootView === 'offline' && !hasActiveSearch && activeFilterCount === 0 && (
                    <section className="tcloud-collection-heading"><div className="tcloud-workspace-icon is-success"><HardDrive className="h-5 w-5" /></div><div><h2>Neste computador</h2><p>Arquivos mantidos localmente para abrir mesmo sem internet.</p></div><span>{displayedFiles.length} item(ns)</span></section>
                )}
                {activeFolderId === null && rootView === 'vault' && !hasActiveSearch && activeFilterCount === 0 && (
                    <ExplorerVaultHeader itemCount={displayedFiles.length} />
                )}
                {showUtilityWorkspace && (
                    <ExplorerUtilityWorkspace view={rootView === 'trash' ? 'trash' : 'sync'} searchTerm={searchTerm} onOpenSyncDetails={() => setShowTCloudSyncCenter(true)} />
                )}

                {(hasActiveSearch || activeFilterCount > 0) && (
                    <div className="px-5 pb-0 pt-3">
                        <h2 className="text-ui font-medium text-app-text-secondary">
                            {hasActiveSearch ? (
                                <>Resultados para <span className="text-app-accent">"{searchTerm.trim()}"</span></>
                            ) : (
                                <>Arquivos filtrados</>
                            )}
                            <span className="ms-2 text-app-text-tertiary">· {displayedFiles.length} encontrados</span>
                        </h2>
                    </div>
                )}
                {rootView === 'problems' && activeFolderId === null && !hasActiveSearch && activeFilterCount === 0 && (
                    <section className={`tcloud-problems-overview ${problemCount > 0 ? 'has-problems' : 'is-clear'}`}>
                        <div className="tcloud-problems-overview-icon">
                            {problemCount > 0 ? <CircleAlert className="h-5 w-5" /> : <CheckCircle2 className="h-5 w-5" />}
                        </div>
                        <div className="min-w-0 flex-1">
                            <h3>{problemCount > 0 ? `${problemCount} ${problemCount === 1 ? 'item precisa' : 'itens precisam'} de atenção` : 'Nenhum problema'}</h3>
                            <p>{problemCount > 0 ? 'O TCloud já tentou corrigir automaticamente. Você pode tentar novamente ou abrir a sincronização para ver cada item.' : 'O TCloud está cuidando da sincronização automaticamente. Quando algo realmente precisar de você, aparecerá aqui.'}</p>
                        </div>
                        <div className="tcloud-problems-overview-actions">
                            {historicalProblemCount > 0 && <button type="button" onClick={() => void handleCleanupHistoricalProblems()} title="Remove apenas falhas antigas que já foram substituídas por tentativas mais novas"><CheckCircle2 className="h-3.5 w-3.5" /> Limpar {historicalProblemCount} antigo(s)</button>}
                            {problemCount > 0 && <button type="button" className="is-primary" onClick={() => void handleRetryAllProblems()}><RefreshCw className="h-3.5 w-3.5" /> Tentar novamente</button>}
                            <button type="button" onClick={() => { setSelectedForum(null); setActiveFolderId(null); setRootView('sync'); }}>Abrir sincronização</button>
                        </div>
                    </section>
                )}
                {showFileExplorer && (
                <FileExplorer
                    folders={folders}
                    files={displayedFiles}
                    loading={(isLoading && allFiles.length === 0) || (isSearching && searchResults.length === 0 && normalizedSearchTerm.length > 2)}
                    error={error}
                    viewMode={viewMode}
                    selectedIds={selectedIds}
                    activeFolderId={activeFolderId}
                    setActiveFolderId={setActiveFolderId}
                    onFileClick={handleFileClick}
                    onDelete={(id) => void handleExplorerDelete(id)}
                    onDownload={(id, name) => queueDownload(id, name, activeFolderId)}
                    onPreview={handlePreview}
                    onManualUpload={handleUploadFromCurrentView}
                    onFolderUpload={handleFolderUploadFromCurrentView}
                    onCreateFolderRequest={beginCreateFolder}
                    onRefresh={handleSyncFolders}
                    onCopySelection={handleExplorerCopy}
                    onCutSelection={handleExplorerCut}
                    onPaste={explorerClipboard ? () => void handleExplorerPaste() : undefined}
                    showFolderUpload={settings.zipFolders}
                    onToggleSelection={handleToggleSelection}
                    onShare={setShareFile}
                    onRename={handleRename}
                    onFileMove={handleFileMove}
                    onDetails={(file) => { setSelectedIds([file.id]); setShowDetailsPanel(true); }}
                    onKeepOffline={async (file) => {
                        try {
                            const folderId = file.folder_id ?? activeFolderId;
                            const result = await setCfapiItemAvailability(file.id, folderId, true);
                            queryClient.setQueryData<TelegramFile[]>(['files', activeFolderId], (current = []) => current.map((item) => item.id === file.id ? { ...item, local_status: 'cached', is_pinned: true } : item));
                            toast.success(result.detail);
                        } catch (error) {
                            toast.error(`Não foi possível deixar o arquivo offline: ${String(error)}`);
                        }
                    }}
                    onFreeSpace={async (file) => {
                        try {
                            const folderId = file.folder_id ?? activeFolderId;
                            const result = await setCfapiItemAvailability(file.id, folderId, false);
                            await invoke('cmd_delete_preview_for_message', { messageId: file.id, folderId }).catch(() => {});
                            await invoke('cmd_delete_image_thumbnail', { messageId: file.id, folderId }).catch(() => {});
                            queryClient.setQueryData<TelegramFile[]>(['files', activeFolderId], (current = []) => current.map((item) => item.id === file.id ? { ...item, local_status: 'cloud_only', is_pinned: false } : item));
                            toast.success(result.detail);
                        } catch (error) {
                            toast.error(`Não foi possível liberar espaço: ${String(error)}`);
                        }
                    }}
                    onVersion={async (file) => {
                        await invoke('tcloud_pro_version_add', { messageId: file.id, folderId: file.folder_id ?? activeFolderId, name: file.name, size: file.size, note: 'Snapshot pelo menu de contexto' });
                        toast.success('VersÃ£o registrada.');
                    }}
                    onBulkDownload={handleBulkDownload}
                    onBulkDelete={handleBulkDelete}
                    onBulkShare={handleBulkShare}
                    onBulkMove={() => { setMoveFileTarget(null); setShowMoveModal(true); }}
                    onBulkKeepOffline={() => void handleBulkAvailability(true)}
                    onBulkFreeSpace={() => void handleBulkAvailability(false)}
                    cardScale={cardScale}
                    sortField={sortField}
                    sortDirection={sortDirection}
                    onSortChange={handleSortChange}
                />
                )}
                {!showUtilityWorkspace && !showHomeDashboard && <ExplorerStatusBar
                    itemCount={displayedFiles.length}
                    selectedCount={selectedIds.length}

                    selectedSize={selectedStatusSize}
                    isConnected={isConnected}
                    computerConnected={Boolean(explorerConnection?.provider_connected)}
                    isSyncing={isSyncing}
                    syncingCount={syncingCount}
                    problemCount={problemCount}
                    onStatusClick={() => setShowTCloudSyncCenter(true)}
                    viewMode={viewMode}
                    onViewModeChange={setViewMode}
                    cardScale={cardScale}
                    onCardScaleChange={setCardScale}
                />}
            </main>

            <Suspense fallback={<LazyOverlayFallback />}>
            {previewFile && (
                <PreviewModal
                    file={previewFile}
                    activeFolderId={activeFolderId}
                    onClose={() => setPreviewFile(null)}
                    onNext={handleNextPreview}
                    onPrev={handlePrevPreview}
                    currentIndex={previewContextIndex}
                    totalItems={previewContextFiles.length}
                    nextFile={previewNeighbors.nextFile}
                    prevFile={previewNeighbors.prevFile}
                />
            )}

            {archiveViewFile && (
                <ArchiveViewerModal
                    file={archiveViewFile}
                    activeFolderId={activeFolderId}
                    folders={folders}
                    onClose={() => setArchiveViewFile(null)}
                    onNext={handleNextPreview}
                    onPrev={handlePrevPreview}
                    currentIndex={previewContextIndex}
                    totalItems={previewContextFiles.length}
                    nextFile={previewNeighbors.nextFile}
                    prevFile={previewNeighbors.prevFile}
                />
            )}
            </Suspense>


            <TransferCenter
                uploads={uploadQueue}
                downloads={downloadQueue}
                onClearUploads={() => setUploadQueue(q => q.filter(i => i.status !== 'success' && i.status !== 'error' && i.status !== 'cancelled'))}
                onCancelUploads={cancelUploads}
                onCancelUpload={cancelUploadItem}
                onRetryUpload={retryUploadItem}
                onSetUploadPriority={setUploadPriority}
                onClearDownloads={clearDownloads}
                onCancelDownloads={cancelDownloads}
                onCancelDownload={cancelDownloadItem}
                onRetryDownload={retryDownloadItem}
                onOpenDetails={() => setShowTCloudSyncCenter(true)}
            />

            <Suspense fallback={<LazyOverlayFallback />}>
            {showSettings && (
                <SettingsModal
                    isOpen
                    onClose={() => setShowSettings(false)}
                    onOpenOperations={() => { setShowSettings(false); setShowTCloudSyncCenter(true); }}
                    onOpenAdvanced={() => { setShowSettings(false); setAdvancedInitialTab('index'); setShowAdvancedCenter(true); }}
                    onOpenSetup={() => setShowTCloudSetup(true)}
                    isConnected={isConnected}
                />
            )}

            {showTCloudDashboard && (
                <TCloudDashboardModal
                    isOpen
                    onClose={() => setShowTCloudDashboard(false)}
                    onOpenSettings={() => setShowSettings(true)}
                />
            )}

            {showTCloudSyncCenter && (
                <TCloudSyncCenterModal
                    isOpen
                    onClose={() => setShowTCloudSyncCenter(false)}
                />
            )}
            {showTCloudOperations && (
                <TCloudOperationsCenterModal
                    isOpen
                    onClose={() => setShowTCloudOperations(false)}
                />
            )}
            {showForumManager && (
                <TCloudForumManagerModal isOpen onClose={() => setShowForumManager(false)} onChanged={() => void handleSyncFolders(true)} />
            )}
            {showDetailsPanel && selectedIds.length === 1 && (() => {
                const detailFile = displayedFiles.find((file) => file.id === selectedIds[0] && file.type !== 'folder');
                if (!detailFile) return null;
                return (
                    <FileDetailsPanel
                        file={detailFile}
                        folderName={currentFolderName}
                        onClose={() => setShowDetailsPanel(false)}
                        onShare={() => setShareFile(detailFile)}
                        onVersion={async () => {
                            await invoke('tcloud_pro_version_add', { messageId: detailFile.id, folderId: detailFile.folder_id ?? activeFolderId, name: detailFile.name, size: detailFile.size, note: 'Snapshot pelo painel de detalhes' });
                            toast.success('VersÃ£o registrada.');
                        }}
                        onOffline={async () => {
                            try {
                                const folderId = detailFile.folder_id ?? activeFolderId;
                                const result = await setCfapiItemAvailability(detailFile.id, folderId, true);
                                queryClient.setQueryData<TelegramFile[]>(['files', activeFolderId], (current = []) => current.map((item) => item.id === detailFile.id ? { ...item, local_status: 'cached', is_pinned: true } : item));
                                toast.success(result.detail);
                            } catch (error) {
                                toast.error(`Não foi possível deixar o arquivo offline: ${String(error)}`);
                            }
                        }}
                        onDownload={() => queueDownload(detailFile.id, detailFile.name, detailFile.folder_id ?? activeFolderId)}
                        onPreview={() => handlePreview(detailFile, displayedFiles)}
                    />
                );
            })()}

            {showAdvancedCenter && (
                <TCloudAdvancedCenterModal
                    isOpen
                    onClose={() => setShowAdvancedCenter(false)}
                    files={displayedFiles}
                    folders={folders}
                    activeFolderId={activeFolderId}
                    selectedIds={selectedIds}
                    onSelectionChange={setSelectedIds}
                    onRefresh={() => queryClient.invalidateQueries({ queryKey: ['files'] })}
                    onOpenSyncCenter={() => setShowTCloudSyncCenter(true)}
                    onOpenShare={(file) => setShareFile(file)}
                    onSearch={(query) => setSearchTerm(query)}
                    onRunBackup={handleRunBackup}
                    initialTab={advancedInitialTab}
                />
            )}

            {showTCloudSetup && (
                <TCloudSetupWizard
                    isOpen
                    onClose={() => setShowTCloudSetup(false)}
                    onCompleted={() => setShowTCloudDashboard(true)}
                />
            )}
{shareFile && (
                <ShareDialog
                    file={shareFile}
                    onClose={() => setShareFile(null)}
                />
            )}
</Suspense>

            {/* Bulk Share Results Modal */}
            {bulkShareLinks && (
                <div
                    className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60 backdrop-blur-sm"
                    onClick={() => setBulkShareLinks(null)}
                >
                    <div
                        className="bg-telegram-surface border border-telegram-border rounded-xl w-[500px] max-h-[70vh] shadow-2xl overflow-hidden flex flex-col animate-in fade-in zoom-in-95 duration-150"
                        onClick={e => e.stopPropagation()}
                    >
                        <div className="p-4 border-b border-telegram-border flex items-center justify-between">
                            <h3 className="text-telegram-text font-medium flex items-center gap-2">
                                <Link className="w-5 h-5 text-telegram-primary" />
                                {bulkShareLinks.length} Share Link{bulkShareLinks.length !== 1 ? 's' : ''}
                            </h3>
                            <button onClick={() => setBulkShareLinks(null)} className="text-telegram-subtext hover:text-telegram-text">
                                <X className="w-5 h-5" />
                            </button>
                        </div>

                        {bulkShareLoading ? (
                            <div className="flex flex-col items-center justify-center py-16 space-y-3">
                                <Loader2 className="w-8 h-8 text-telegram-primary animate-spin" />
                                <p className="text-sm text-telegram-subtext">Generating share links...</p>
                            </div>
                        ) : (
                            <div className="flex-1 overflow-y-auto p-4 space-y-2 min-h-0">
                                {bulkShareLinks.map(({ file, link }) => {
                                    const isCopied = bulkShareCopied.has(link);
                                    return (
                                        <div
                                            key={file.id}
                                            className="p-3 rounded-lg bg-telegram-hover/30 border border-telegram-border/30 space-y-2"
                                        >
                                            <p className="text-xs font-semibold text-telegram-text truncate">{file.name}</p>
                                            <div className="flex gap-2">
                                                <input
                                                    type="text"
                                                    readOnly
                                                    value={link}
                                                    className="flex-1 bg-telegram-bg border border-telegram-border rounded-lg px-2.5 py-1.5 text-xs text-telegram-text focus:outline-none select-all truncate"
                                                />
                                                <button
                                                    onClick={() => handleCopyBulkLink(link)}
                                                    className={`px-2.5 py-1.5 rounded-lg border flex items-center justify-center transition-all flex-shrink-0 ${
                                                        isCopied
                                                            ? 'bg-emerald-500 border-emerald-500 text-white'
                                                            : 'bg-telegram-hover border-telegram-border text-telegram-text hover:bg-white/10'
                                                    }`}
                                                >
                                                    {isCopied ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
                                                </button>
                                                {typeof navigator !== 'undefined' && typeof navigator.share === 'function' && (
                                                    <button
                                                        onClick={() => nativeShareOrCopy(file.name, file.sizeStr, link, () => handleCopyBulkLink(link))}
                                                        className="px-2.5 py-1.5 rounded-lg bg-telegram-primary/20 hover:bg-telegram-primary/30 text-telegram-primary border border-telegram-primary/30 transition-all flex items-center justify-center flex-shrink-0"
                                                    >
                                                        <Share2 className="w-3.5 h-3.5" />
                                                    </button>
                                                )}
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        )}

                        <button
                            onClick={() => setBulkShareLinks(null)}
                            className="w-full px-4 py-2.5 border-t border-telegram-border bg-telegram-hover/20 hover:bg-telegram-hover/40 text-telegram-text text-sm font-medium transition-colors"
                        >
                            Done
                        </button>
                    </div>
                </div>
            )}
                <DragOverlay dropAnimation={null}>
                    {internalDrag && (
                        <div className={`tcloud-internal-drag-card ${internalDragTarget ? 'has-target' : ''}`}>
                            <div className="tcloud-internal-drag-main">
                                <Files className="h-4 w-4 shrink-0 text-app-accent" />
                                <span className="truncate">{internalDrag.fileIds.length > 1 ? `${internalDrag.fileIds.length} itens` : internalDrag.label}</span>
                                {internalDrag.fileIds.length > 1 && (
                                    <span className="tcloud-internal-drag-count">{internalDrag.fileIds.length}</span>
                                )}
                            </div>
                            {internalDragTarget && (
                                <div className="tcloud-internal-drag-target"><ArrowRight className="h-3.5 w-3.5" /><span>Mover para {internalDragTarget}</span></div>
                            )}
                        </div>
                    )}
                </DragOverlay>
            </div>
        </DndContext>
    );
}
