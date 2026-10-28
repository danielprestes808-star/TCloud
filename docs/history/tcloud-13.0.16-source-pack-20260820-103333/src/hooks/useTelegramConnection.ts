import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { load, type Store } from '@tauri-apps/plugin-store';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useConfirm } from '../context/ConfirmContext';
import { TelegramFolder, FolderInviteInfo, FolderGroup } from '../types';
import { useNetworkStatus } from './useNetworkStatus';
import { clearImageMemoryCaches } from '../services/imagePreviewCache';
import { createTCloudForumTopic, listTCloudForumTree } from '../services/tcloudForums';
import { isTCloudUiBackgroundReady } from '../services/tcloudWindowResume';

export function useTelegramConnection(onLogoutParent: () => void) {
    const queryClient = useQueryClient();
    const { confirm } = useConfirm();

    const [folders, setFolders] = useState<TelegramFolder[]>([]);
    const [groups, setGroups] = useState<FolderGroup[]>([]);
    const [activeFolderId, setActiveFolderId] = useState<number | null>(null);
    const [store, setStore] = useState<Store | null>(null);
    const [isSyncing, setIsSyncing] = useState(false);
    const [isConnected, setIsConnected] = useState(true);

    const networkIsOnline = useNetworkStatus();
    const handleSyncFoldersRef = useRef<((silentParam?: boolean | unknown) => Promise<void>) | null>(null);
    const initialSyncStartedRef = useRef(false);
    const syncInFlightRef = useRef<Promise<void> | null>(null);

    // Fetch groups list from DB
    const fetchGroups = useCallback(async () => {
        try {
            const list = await invoke<FolderGroup[]>('cmd_get_groups');
            setGroups((current) => JSON.stringify(current) === JSON.stringify(list) ? current : list);
        } catch (e) {
            console.error("Failed to fetch folder groups:", e);
        }
    }, []);

    // Load persisted store and restore saved folders.
    useEffect(() => {
        const initStore = async () => {
            try {
                let _store = await load('config.json');
                const checkId = await _store.get<string>('api_id');
                if (!checkId) {
                    _store = await load('settings.json');
                }
                // Fetch local-first SQLite enriched folders and keep a snapshot
                // so a deleted/stale startup destination is never restored.
                let startupFolders: TelegramFolder[] = [];
                try {
                    const dbFolders = await invoke<TelegramFolder[]>('cmd_get_enriched_folders');
                    if (dbFolders && dbFolders.length > 0) {
                        startupFolders = dbFolders;
                        setFolders(dbFolders);
                    } else {
                        const savedFolders = await _store.get<TelegramFolder[]>('folders');
                        startupFolders = savedFolders ?? [];
                        if (savedFolders) setFolders(savedFolders);
                    }
                } catch {
                    const savedFolders = await _store.get<TelegramFolder[]>('folders');
                    startupFolders = savedFolders ?? [];
                    if (savedFolders) setFolders(savedFolders);
                }

                // Fetch local-first SQLite groups
                try {
                    const list = await invoke<FolderGroup[]>('cmd_get_groups');
                    setGroups(list);
                } catch (e) {
                    console.error("Failed to load groups:", e);
                }

                const savedActiveFolderId = await _store.get<number | null>('activeFolderId');
                if (savedActiveFolderId !== undefined) {
                    const stillExists = savedActiveFolderId === null
                        || startupFolders.some((folder) => folder.id === savedActiveFolderId);
                    const restored = stillExists ? savedActiveFolderId : null;
                    setActiveFolderId(restored);
                    if (!stillExists) {
                        await _store.set('activeFolderId', null);
                        await _store.save();
                    }
                }

                // Enable file queries only after the persisted folder has been restored.
                // Otherwise the dashboard briefly loads Saved Messages first, then starts
                // a second overlapping request for the actual startup folder.
                setStore(_store);
                setIsConnected(true);
            } catch {
                // store not available
            }
        };
        initStore();
    }, [queryClient]);

    // If a folder/topic is removed while TCloud is open (or disappears from
    // Telegram), never leave the dashboard pointing at that stale id.
    useEffect(() => {
        if (!store || activeFolderId === null || isSyncing) return;
        if (folders.some((folder) => folder.id === activeFolderId)) return;

        setActiveFolderId(null);
        void (async () => {
            try {
                await store.set('activeFolderId', null);
                await store.save();
            } catch { /* best effort */ }
        })();
    }, [store, activeFolderId, folders, isSyncing]);

    // Consolidated mount-sync + visibility-change listener
    useEffect(() => {
        if (!store || !isConnected) return;

        const syncAndRefresh = () => {
            if (!handleSyncFoldersRef.current) return Promise.resolve();
            if (syncInFlightRef.current) return syncInFlightRef.current;

            const request = (async () => {
                await handleSyncFoldersRef.current?.(true);
                await queryClient.invalidateQueries({
                    queryKey: ['files', activeFolderId],
                    exact: true,
                });
            })().finally(() => {
                syncInFlightRef.current = null;
            });
            syncInFlightRef.current = request;
            return request;
        };

        // Run background reconciliation when the WebView has breathing room. Cached
        // SQLite state is already on screen, so Telegram reconciliation should not
        // compete with startup paint, scrolling or a click immediately after focus.
        let fallbackTimer: number | null = null;
        let idleId: number | null = null;
        const scheduleIdleSync = (delayMs = 250) => {
            if (fallbackTimer !== null || idleId !== null) return;
            const run = () => {
                fallbackTimer = null;
                idleId = null;
                if (document.visibilityState === 'visible') void syncAndRefresh();
            };
            fallbackTimer = window.setTimeout(() => {
                fallbackTimer = null;
                if ('requestIdleCallback' in window) {
                    idleId = window.requestIdleCallback(run, { timeout: 1800 });
                } else {
                    run();
                }
            }, delayMs);
        };

        // React Strict Mode remounts effects in development. Keep the initial refresh
        // single-shot so it cannot start two Telegram file streams for the same folder.
        if (!initialSyncStartedRef.current) {
            initialSyncStartedRef.current = true;
            scheduleIdleSync(700);
        }

        const handleVisibilityChange = () => {
            if (document.visibilityState === 'visible') scheduleIdleSync(180);
        };

        document.addEventListener('visibilitychange', handleVisibilityChange);
        return () => {
            document.removeEventListener('visibilitychange', handleVisibilityChange);
            if (fallbackTimer !== null) window.clearTimeout(fallbackTimer);
            if (idleId !== null && 'cancelIdleCallback' in window) window.cancelIdleCallback(idleId);
        };
    }, [store, isConnected, queryClient, activeFolderId]);

    // Remote reconciliation loop. The local watcher handles PC-side changes; this
    // lightweight poll refreshes Telegram-side changes while the app is visible,
    // completing the bidirectional sync path without hammering the API.
    useEffect(() => {
        if (!store || !isConnected) return;
        const intervalMs = Math.max(15_000, Number(localStorage.getItem('tcloud:remote-sync-poll-ms') || '30000'));
        const tick = async () => {
            if (
                document.visibilityState !== 'visible'
                || syncInFlightRef.current
                || !isTCloudUiBackgroundReady(0, 900)
            ) return;
            const request = (async () => {
                await handleSyncFoldersRef.current?.(true);
                await queryClient.invalidateQueries({
                    queryKey: ['files', activeFolderId],
                    exact: true,
                });
            })().finally(() => { syncInFlightRef.current = null; });
            syncInFlightRef.current = request;
            await request;
        };
        let idleId: number | null = null;
        let fallbackTimer: number | null = null;
        const scheduleTick = () => {
            if (document.visibilityState !== 'visible' || idleId !== null || fallbackTimer !== null) return;
            fallbackTimer = window.setTimeout(() => {
                fallbackTimer = null;
                const run = () => { idleId = null; void tick(); };
                if ('requestIdleCallback' in window) idleId = window.requestIdleCallback(run, { timeout: 2200 });
                else run();
            }, 120);
        };
        const timer = window.setInterval(scheduleTick, intervalMs);
        return () => {
            window.clearInterval(timer);
            if (fallbackTimer !== null) window.clearTimeout(fallbackTimer);
            if (idleId !== null && 'cancelIdleCallback' in window) window.cancelIdleCallback(idleId);
        };
    }, [store, isConnected, queryClient, activeFolderId]);

    useEffect(() => {
        setIsConnected(networkIsOnline);
    }, [networkIsOnline]);

    const handleLogout = async () => {
        if (!await confirm({ title: "Sign Out", message: "Are you sure you want to sign out? This will disconnect your active session.", confirmText: "Sign Out", variant: 'danger' })) return;

        try {
            await invoke('cmd_logout');
            await invoke('cmd_clean_cache');
            clearImageMemoryCaches();
            if (store) {
                await store.delete('api_id');
                await store.delete('api_hash');
                await store.delete('folders');
                await store.save();
            }
            onLogoutParent();
        } catch {
            toast.error("Error signing out");
            onLogoutParent();
        }
    };

    const handleSyncFolders = async (silentParam?: boolean | unknown) => {
        const silent = silentParam === true;
        if (!store) return;
        if (!silent) setIsSyncing(true);
        try {
            const foundFolders = await invoke<TelegramFolder[]>('cmd_scan_folders');
            // Refresh the first-level TCloud folder catalog directly from Telegram.
            // This runs even when the sidebar is collapsed, keeping Explorer in sync.
            try { await listTCloudForumTree(); } catch (error) { console.warn('Could not refresh TCloud folder tree', error); }
            const foldersChanged = JSON.stringify(foundFolders) !== JSON.stringify(folders);
            if (foldersChanged) {
                setFolders(foundFolders);
                await store.set('folders', foundFolders);
                await store.save();
            }
            await fetchGroups();
            window.dispatchEvent(new Event('tcloud-forums-changed'));
            if (!silent) {
                toast.success("Pastas atualizadas.");
            }
        } catch (e) {
            if (!silent) {
                toast.error("Sync failed: " + e);
            }
        } finally {
            if (!silent) setIsSyncing(false);
        }
    };

    // Keep the ref in sync
    handleSyncFoldersRef.current = handleSyncFolders;

    // TCloud 7.3.3 compatibility migration.
    //
    // Modern installations already have a persisted folder catalog and should
    // never perform a full Telegram folder scan during startup.
    //
    // Only an installation with no known folders still needs the legacy scan,
    // and even then it runs well after the UI/current folder have settled.
    useEffect(() => {
        if (!isConnected || !store) return;

        const migrationKey = 'tcloud:legacy-folder-index-v733';

        if (localStorage.getItem(migrationKey) === 'done') return;

        if (folders.length > 0) {
            localStorage.setItem(migrationKey, 'done');
            return;
        }

        const timer = window.setTimeout(() => {
            void handleSyncFoldersRef.current?.(true)
                .then(() => localStorage.setItem(migrationKey, 'done'))
                .catch((error) =>
                    console.warn(
                        'Legacy TCloud folder import failed:',
                        error
                    )
                );
        }, 15_000);

        return () => window.clearTimeout(timer);
    }, [isConnected, store, folders.length]);


    const handleCreateFolder = async (name: string) => {
        if (!store) return;
        try {
            const newFolder = await createTCloudForumTopic(name);
            const updated = [...folders, newFolder];
            setFolders(updated);
            await store.set('folders', updated);
            await store.save();
            window.dispatchEvent(new Event('tcloud-forums-changed'));
            toast.success(`Pasta "${name}" criada.`);
        } catch (e) {
            toast.error("N?o foi poss?vel criar a pasta: " + String(e));
            throw e;
        }
    };

    const handleFolderDelete = async (folderId: number, folderName: string) => {
        if (!await confirm({
            title: "Excluir pasta",
            message: `Excluir "${folderName}"?\nEla também será removida dos seus locais sincronizados.`,
            confirmText: "Excluir",
            variant: 'danger'
        })) return;

        try {
            await invoke('cmd_delete_folder', { folderId });
            const updated = folders.filter(f => f.id !== folderId);
            setFolders(updated);
            if (store) {
                await store.set('folders', updated);
                await store.save();
            }
            if (activeFolderId === folderId) setActiveFolderId(null);
            window.dispatchEvent(new Event('tcloud-forums-changed'));
            toast.success(`Pasta "${folderName}" excluída.`);
        } catch (e: unknown) {
            const errStr = String(e);
            if (errStr.includes("not found")) {
                if (await confirm({
                    title: "Pasta não encontrada",
                    message: `A pasta "${folderName}" não está mais disponível. Deseja remover a referência antiga do TCloud?`,
                    confirmText: "Remover",
                    variant: 'info'
                })) {
                    const updated = folders.filter(f => f.id !== folderId);
                    setFolders(updated);
                    if (store) {
                        await store.set('folders', updated);
                        await store.save();
                    }
                    if (activeFolderId === folderId) setActiveFolderId(null);
                }
            } else {
                toast.error(`N?o foi poss?vel excluir a pasta: ${String(e)}`);
            }
        }
    };

    const handleFolderRename = async (
        folderId: number,
        oldNameOrNewName: string,
        newNameOverride?: string
    ) => {
        // 14.28:
        // Alguns componentes chamam (id, novoNome) e outros ainda usam
        // (id, nomeAntigo, novoNome). Aceitamos os dois formatos.
        const currentName =
            newNameOverride !== undefined
                ? oldNameOrNewName
                : (folders.find(f => f.id === folderId)?.name ?? '');

        const requestedName =
            (newNameOverride ?? oldNameOrNewName).trim();

        // Regra do Windows: ponto/espa?o n?o podem ficar no final.
        const newName = requestedName.replace(/[. ]+$/g, '');

        if (!newName) {
            toast.error('Digite um nome v\u00e1lido.');
            return;
        }

        if (currentName && newName === currentName) {
            if (requestedName !== currentName) {
                toast.info(
                    'Ponto ou espa\u00e7o no final do nome foi removido.'
                );
            }
            return;
        }

        try {
            const renamed = await invoke<boolean>(
                'cmd_rename_folder',
                { folderId, newName }
            );

            if (!renamed) {
                throw new Error(
                    'A altera\u00e7\u00e3o n\u00e3o foi confirmada.'
                );
            }

            const updated = folders.map(f =>
                f.id === folderId
                    ? { ...f, name: newName }
                    : f
            );

            setFolders(updated);

            if (store) {
                await store.set('folders', updated);
                await store.save();
            }

            window.dispatchEvent(
                new Event('tcloud-forums-changed')
            );

            toast.success(`Renomeado para "${newName}".`);
        } catch (e) {
            toast.error(
                'N\u00e3o foi poss\u00edvel renomear: ' + String(e)
            );
        }
    };

    const handleFolderToggleVisibility = async (folderId: number, makePublic: boolean, desiredUsername?: string) => {
        if (!makePublic) {
            const confirmed = await confirm({
                title: "Make Private",
                message: "Making this channel private will remove its public username. Any shared t.me links will stop working immediately.",
                confirmText: "Make Private",
                variant: 'danger'
            });
            if (!confirmed) return;
        }
        try {
            const updated = await invoke<TelegramFolder>('cmd_toggle_folder_visibility', {
                folderId,
                makePublic,
                desiredUsername: desiredUsername || null,
            });
            const newFolders = folders.map(f =>
                f.id === folderId ? { ...f, username: updated.username, is_public: updated.is_public } : f
            );
            setFolders(newFolders);
            if (store) {
                await store.set('folders', newFolders);
                await store.save();
            }
            toast.success(makePublic ? 'Channel is now public' : 'Channel is now private');
            return updated;
        } catch (e) {
            toast.error(`N?o foi poss?vel alterar a visibilidade: ${String(e)}`);
            throw e;
        }
    };

    const handleExportFolderInvite = async (folderId: number): Promise<FolderInviteInfo> => {
        try {
            const info = await invoke<FolderInviteInfo>('cmd_export_folder_invite', {
                folderId,
            });
            return info;
        } catch (e) {
            toast.error(`N?o foi poss?vel obter o link: ${String(e)}`);
            throw e;
        }
    };

    const handleSetActiveFolderId = async (id: number | null) => {
        setActiveFolderId(id);
        if (store) {
            await store.set('activeFolderId', id);
            await store.save();
        }
    };

    // Group Management Actions
    const handleCreateGroup = async (name: string, colorHex: string = '#3B82F6') => {
        try {
            const id = await invoke<number>('cmd_create_group', { name, colorHex });
            const newGroup: FolderGroup = { id, name, color_hex: colorHex, display_order: groups.length };
            setGroups(prev => [...prev, newGroup]);
            toast.success(`Grupo "${name}" criado.`);
        } catch (e) {
            toast.error("N?o foi poss?vel criar o grupo: " + String(e));
        }
    };

    const handleDeleteGroup = async (groupId: number) => {
        try {
            await invoke('cmd_delete_group', { groupId });
            setGroups(prev => prev.filter(g => g.id !== groupId));
            setFolders(prev => prev.map(f => f.group_id === groupId ? { ...f, group_id: null } : f));
            toast.success("Grupo exclu?do.");
        } catch (e) {
            toast.error("N?o foi poss?vel excluir o grupo: " + String(e));
        }
    };

    const handleUpdateGroup = async (groupId: number, name: string, colorHex: string) => {
        try {
            await invoke('cmd_update_group', { groupId, name, colorHex });
            setGroups(prev => prev.map(g => g.id === groupId ? { ...g, name, color_hex: colorHex } : g));
            toast.success("Group updated.");
        } catch (e) {
            toast.error("N?o foi poss?vel atualizar o grupo: " + String(e));
        }
    };

    const handleAssignFolderToGroup = async (folderId: number, groupId: number | null) => {
        try {
            await invoke('cmd_assign_folder_to_group', { channelId: folderId, groupId });
            setFolders(prev => prev.map(f => f.id === folderId ? { ...f, group_id: groupId } : f));
        } catch (e) {
            toast.error("N?o foi poss?vel mover a pasta para o grupo: " + String(e));
        }
    };

    const handleReorderFolders = async (reordered: TelegramFolder[]) => {
        setFolders(reordered);
        if (store) {
            await store.set('folders', reordered);
            await store.save();
        }
        try {
            await Promise.all(
                reordered.map((folder, index) =>
                    invoke('cmd_update_folder_order', { channelId: folder.id, newOrder: index })
                )
            );
        } catch (e) {
            console.error("Failed to persist folder reordering:", e);
        }
    };

    const handleUpdateGroupOrder = async (reorderedGroups: FolderGroup[]) => {
        setGroups(reorderedGroups);
        try {
            await Promise.all(
                reorderedGroups.map((g, index) =>
                    invoke('cmd_update_group_order', { groupId: g.id, newOrder: index })
                )
            );
        } catch (e) {
            console.error("Failed to persist group order:", e);
        }
    };

    return {
        store,
        folders,
        groups,
        activeFolderId,
        setActiveFolderId: handleSetActiveFolderId,
        isSyncing,
        isConnected,
        handleLogout,
        handleSyncFolders,
        handleCreateFolder,
        handleFolderDelete,
        handleFolderRename,
        handleFolderToggleVisibility,
        handleExportFolderInvite,
        // Group Actions
        handleCreateGroup,
        handleDeleteGroup,
        handleUpdateGroup,
        handleAssignFolderToGroup,
        handleReorderFolders,
        handleUpdateGroupOrder,
        fetchGroups
    };
}
