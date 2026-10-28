import { useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type { LucideIcon } from 'lucide-react';
import {
    Archive,
    ChevronLeft,
    ChevronRight,
    CloudDownload,
    Clock3,
    CircleAlert,
    Activity,
    Copy,
    ExternalLink,
    FileText,
    Folder,
    GalleryVerticalEnd,
    HardDrive,
    Home,
    Image,
    LockKeyhole,
    LogOut,
    MoreHorizontal,
    MoreVertical,
    MoveRight,
    Music2,
    Pencil,
    Settings,
    Star,
    Trash2,
    Video,
    Wrench,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { TelegramFolder, BandwidthStats, FolderGroup } from '../../../types';
import { useSettings } from '../../../context/SettingsContext';
import {
    deleteTCloudMainFolder,
    listTCloudForumTree,
    renameTCloudMainFolder,
    transferTCloudForumTopic,
    type TCloudForumTree,
} from '../../../services/tcloudForums';
import { finalizeCfapiExplorer, getCfapiStatus, openCfapiRoot } from '../../../services/tcloudCompletion';

interface SidebarProps {
    folders: TelegramFolder[];
    groups: FolderGroup[];
    activeFolderId: number | null;
    activeForumId?: number | null;
    setActiveFolderId: (id: number | null) => void;
    onWarmFolder?: (id: number) => void;
    rootView: 'dashboard' | 'saved' | 'recent' | 'favorites' | 'problems' | 'sync' | 'trash' | 'gallery' | 'music' | 'offline' | 'vault';
    onRootViewChange: (view: 'dashboard' | 'saved' | 'recent' | 'favorites' | 'problems' | 'sync' | 'trash' | 'gallery' | 'music' | 'offline' | 'vault') => void;
    onDelete: (id: number, name: string) => void;
    onRename: (id: number, name: string) => void;
    onToggleVisibility: (id: number, name: string, isPublic: boolean) => void;
    onExportInvite: (id: number, name: string) => void;
    onCreate: (name: string) => Promise<void>;
    isConnected: boolean;
    onLogout: () => void;
    bandwidth: BandwidthStats | null;
    onAssignFolderToGroup: (folderId: number, groupId: number | null) => Promise<void>;
    onCreateGroup: (name: string, colorHex: string) => Promise<void>;
    onUpdateGroup: (groupId: number, name: string, colorHex: string) => Promise<void>;
    onDeleteGroup: (groupId: number) => Promise<void>;
    onOpenSettings?: () => void;
    onOpenForums?: () => void;
    onOpenAdvanced?: () => void;
    onOpenForum?: (forum: TCloudForumTree) => void;
    problemCount?: number;
    syncingCount?: number;
}

type TopicMenuState = {
    forumId: number;
    topicId: number;
    topicName: string;
    x: number;
    y: number;
};

type DriveMenuState = {
    folderId: number;
    folderName: string;
    x: number;
    y: number;
};

type MainFolderMenuState = {
    forumId: number;
    forumName: string;
    x: number;
    y: number;
};

type SidebarRenameState = {
    kind: 'main' | 'folder';
    id: number;
    originalName: string;
    value: string;
};

type TopicTransferState = {
    forumId: number;
    topicId: number;
    topicName: string;
    mode: 'move' | 'copy';
};

const DRIVE_ORDER = ['Backup', 'Documentos', 'Fotos', 'Mensagens Salvas', 'Música', 'Vídeos'];

function normalizeName(value: string) {
    return value
        .normalize('NFD')
        .replace(/[\u0300-\u036f]/g, '')
        .toLowerCase();
}

function driveFolderRank(name: string) {
    const normalized = normalizeName(name);
    const index = DRIVE_ORDER.findIndex((candidate) => normalizeName(candidate) === normalized);
    return index === -1 ? DRIVE_ORDER.length : index;
}

function DriveFolderIcon({ name, className = 'h-4 w-4' }: { name: string; className?: string }) {
    const normalized = normalizeName(name);
    if (normalized.includes('foto') || normalized.includes('imagem')) return <Image className={className} />;
    if (normalized.includes('video')) return <Video className={className} />;
    if (normalized.includes('musica') || normalized.includes('audio')) return <Music2 className={className} />;
    if (normalized.includes('document')) return <FileText className={className} />;
    if (normalized.includes('backup') || normalized.includes('compact')) return <Archive className={className} />;
    return <Folder className={className} />;
}

function NavItem({
    icon: Icon,
    label,
    active = false,
    onClick,
    title,
    badge,
    badgeTone = 'neutral',
}: {
    icon: LucideIcon;
    label: string;
    active?: boolean;
    onClick: () => void;
    title?: string;
    badge?: number;
    badgeTone?: 'neutral' | 'danger' | 'busy';
}) {
    return (
        <button
            type="button"
            className={`tcloud-explorer-nav-item ${active ? 'is-active' : ''}`}
            onClick={onClick}
            title={title || label}
        >
            <Icon className="h-4 w-4 shrink-0" />
            <span>{label}</span>
            {typeof badge === 'number' && badge > 0 && <b className={`tcloud-explorer-nav-badge is-${badgeTone}`}>{badge > 99 ? '99+' : badge}</b>}
        </button>
    );
}

export function Sidebar({
    folders,
    activeFolderId,
    activeForumId = null,
    setActiveFolderId,
    onWarmFolder,
    rootView,
    onRootViewChange,
    onDelete,
    onRename,
    isConnected,
    onLogout,
    onOpenSettings,
    onOpenForums,
    onOpenAdvanced,
    onOpenForum,
    problemCount = 0,
    syncingCount = 0,
}: SidebarProps) {
    const { t } = useTranslation();
    const { settings, updateSetting } = useSettings();
    const queryClient = useQueryClient();

    // Explorer self-heal:
    // evita mais de uma tentativa simultânea de reconstruir o registro
    // StorageProvider/SyncRoot do painel de navegação.
    const explorerRepairInFlightRef = useRef(false);
    const { data: forumTree = [] } = useQuery({
        queryKey: ['tcloud-forum-tree'],
        queryFn: listTCloudForumTree,
        enabled: isConnected,
        staleTime: 15_000,
        refetchInterval: 120_000,
        refetchIntervalInBackground: false,
    });
    const [expandedForums, setExpandedForums] = useState<Record<number, boolean>>(() => {
        try {
            const raw = localStorage.getItem('tcloud:expanded-main-folders');
            return raw ? JSON.parse(raw) as Record<number, boolean> : {};
        } catch {
            return {};
        }
    });
    const [topicMenu, setTopicMenu] = useState<TopicMenuState | null>(null);
    const [driveMenu, setDriveMenu] = useState<DriveMenuState | null>(null);
    const [mainFolderMenu, setMainFolderMenu] = useState<MainFolderMenuState | null>(null);
    const [sidebarRename, setSidebarRename] = useState<SidebarRenameState | null>(null);
    const [sidebarRenameBusy, setSidebarRenameBusy] = useState(false);
    const [topicTransfer, setTopicTransfer] = useState<TopicTransferState | null>(null);
    const [transferTargetForumId, setTransferTargetForumId] = useState<number | null>(null);
    const [isTransferringTopic, setIsTransferringTopic] = useState(false);
    const [footerMenuOpen, setFooterMenuOpen] = useState(false);

    // Share the same React Query cache used by the main dashboard. This removes a
    // second independent CFAPI/WinRT polling loop from the sidebar.
    const { data: cfapiStatus = null } = useQuery({
        queryKey: ['cfapi-status'],
        queryFn: getCfapiStatus,
        refetchInterval: 30_000,
        refetchIntervalInBackground: false,
        staleTime: 15_000,
        enabled: isConnected,
    });

    /*
     * Autorrecuperação do ícone/Sync Root do Explorer.
     *
     * A raiz CFAPI pode continuar funcionando mesmo se o Windows perder
     * apenas o registro StorageProvider usado para exibir o TCloud no
     * painel esquerdo. Quando isso acontecer, refazemos somente a camada
     * Shell. Nenhum placeholder, arquivo local ou objeto Telegram é apagado.
     */
    useEffect(() => {
        if (!isConnected || !cfapiStatus?.registered) return;

        const shellHealthy =
            cfapiStatus.shell_registered &&
            cfapiStatus.shell_pinning_enabled;

        if (shellHealthy) {
            explorerRepairInFlightRef.current = false;
            return;
        }

        // A finalização moderna é segura quando a Package Identity existe.
        // Sem identidade, deixamos o fluxo manual/fallback existente cuidar.
        if (!cfapiStatus.package_identity) return;
        if (explorerRepairInFlightRef.current) return;

        explorerRepairInFlightRef.current = true;

        void finalizeCfapiExplorer()
            .then(async (next) => {
                await queryClient.setQueryData(
                    ['cfapi-status'],
                    next
                );

                if (next.shell_registered) {
                    console.info(
                        'TCloud Explorer: registro visual restaurado automaticamente.'
                    );
                }
            })
            .catch((error) => {
                console.warn(
                    'TCloud Explorer: autorreparo do Shell adiado:',
                    error
                );
            })
            .finally(() => {
                // Permite nova tentativa no próximo ciclo de status,
                // caso o Windows ainda não estivesse pronto.
                window.setTimeout(() => {
                    explorerRepairInFlightRef.current = false;
                }, 15_000);
            });
    }, [
        cfapiStatus?.package_identity,
        cfapiStatus?.registered,
        cfapiStatus?.shell_pinning_enabled,
        cfapiStatus?.shell_registered,
        isConnected,
        queryClient,
    ]);

    const libraryFolders = useMemo(
        () => folders
            .filter((folder) => folder.id >= 0 && normalizeName(folder.name) !== 'mensagens salvas')
            .slice()
            .sort((left, right) => {
                const rank = driveFolderRank(left.name) - driveFolderRank(right.name);
                if (rank !== 0) return rank;
                return left.name.localeCompare(right.name, 'pt-BR', { sensitivity: 'base' });
            }),
        [folders],
    );

    const otherFolders = useMemo(() => {
        const visibleNames = new Set<string>();
        for (const main of forumTree) {
            visibleNames.add(normalizeName(main.name));
            for (const child of main.topics) visibleNames.add(normalizeName(child.name));
        }
        return libraryFolders.filter((folder) => !visibleNames.has(normalizeName(folder.name)));
    }, [libraryFolders, forumTree]);

    useEffect(() => {
        setExpandedForums((current) => {
            const next: Record<number, boolean> = {};
            for (const forum of forumTree) {
                const containsActiveFolder = forum.topics.some((topic) => topic.id === activeFolderId);
                next[forum.id] = containsActiveFolder ? true : (current[forum.id] ?? false);
            }
            return next;
        });
    }, [forumTree, activeFolderId]);

    useEffect(() => {
        const changed = () => void queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] });
        window.addEventListener('tcloud-forums-changed', changed);
        return () => window.removeEventListener('tcloud-forums-changed', changed);
    }, [queryClient]);


    useEffect(() => {
        try {
            localStorage.setItem('tcloud:expanded-main-folders', JSON.stringify(expandedForums));
        } catch { /* optional UI preference */ }
    }, [expandedForums]);

    useEffect(() => {
        const closeMenus = () => {
            setTopicMenu(null);
            setDriveMenu(null);
            setMainFolderMenu(null);
            setFooterMenuOpen(false);
        };
        window.addEventListener('mousedown', closeMenus);
        window.addEventListener('resize', closeMenus);
        return () => {
            window.removeEventListener('mousedown', closeMenus);
            window.removeEventListener('resize', closeMenus);
        };
    }, []);

    // File/folder keyboard shortcuts are handled by the main Explorer surface.
    // Keeping a second global F2/Delete listener here caused duplicate actions.


    const beginTopicTransfer = (forumId: number, topicId: number, topicName: string, mode: 'move' | 'copy') => {
        const target = forumTree.find((forum) => mode === 'copy' || forum.id !== forumId)?.id ?? null;
        setTopicMenu(null);
        setTransferTargetForumId(target);
        setTopicTransfer({ forumId, topicId, topicName, mode });
    };

    const executeTopicTransfer = async () => {
        if (!topicTransfer || transferTargetForumId === null) return;
        setIsTransferringTopic(true);
        try {
            await transferTCloudForumTopic(topicTransfer.topicId, transferTargetForumId, topicTransfer.mode === 'move');
            setTopicTransfer(null);
            setTransferTargetForumId(null);
            window.dispatchEvent(new Event('tcloud-forums-changed'));
            await queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] });
        } catch (error) {
            window.alert(`Não foi possível ${topicTransfer.mode === 'move' ? 'mover' : 'copiar'} a pasta: ${String(error)}`);
        } finally {
            setIsTransferringTopic(false);
        }
    };

    const showTopicMenuAt = (forumId: number, topicId: number, topicName: string, x: number, y: number) => {
        setDriveMenu(null);
        setMainFolderMenu(null);
        setTopicMenu({ forumId, topicId, topicName, x, y });
    };

    const showDriveMenuAt = (folderId: number, folderName: string, x: number, y: number) => {
        setTopicMenu(null);
        setMainFolderMenu(null);
        setDriveMenu({ folderId, folderName, x, y });
    };

    const showMainFolderMenuAt = (forumId: number, forumName: string, x: number, y: number) => {
        setTopicMenu(null);
        setDriveMenu(null);
        setMainFolderMenu({ forumId, forumName, x, y });
    };

    const startSidebarRename = (kind: 'main' | 'folder', id: number, name: string) => {
        setTopicMenu(null);
        setDriveMenu(null);
        setMainFolderMenu(null);
        setSidebarRename({ kind, id, originalName: name, value: name });
    };

    const submitSidebarRename = async () => {
        if (!sidebarRename || sidebarRenameBusy) return;
        const clean = sidebarRename.value.trim();
        if (!clean || clean === sidebarRename.originalName) {
            setSidebarRename(null);
            return;
        }
        setSidebarRenameBusy(true);
        try {
            if (sidebarRename.kind === 'main') {
                await renameTCloudMainFolder(sidebarRename.id, clean);
            } else {
                await Promise.resolve(onRename(sidebarRename.id, clean));
            }
            window.dispatchEvent(new Event('tcloud-forums-changed'));
            await queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] });
            toast.success(`Renomeado para "${clean}".`);
            setSidebarRename(null);
        } catch (error) {
            toast.error(`Não foi possível renomear: ${String(error)}`);
        } finally {
            setSidebarRenameBusy(false);
        }
    };

    const removeMainFolder = async (forumId: number, forumName: string) => {
        setMainFolderMenu(null);
        if (!window.confirm(`Excluir a pasta "${forumName}"?

Ela precisa estar vazia.`)) return;
        try {
            await deleteTCloudMainFolder(forumId);
            window.dispatchEvent(new Event('tcloud-forums-changed'));
            await queryClient.invalidateQueries({ queryKey: ['tcloud-forum-tree'] });
            toast.success(`Pasta "${forumName}" excluída.`);
        } catch (error) {
            const message = String(error);
            if (message.includes('TCLOUD_FOLDER_NOT_EMPTY')) {
                toast.error('Esta pasta ainda contém arquivos ou subpastas. Mova ou exclua o conteúdo primeiro.');
            } else {
                toast.error(`Não foi possível excluir a pasta: ${message}`);
            }
        }
    };

    if (settings.sidebarCollapsed) {
        return (
            <aside className="tcloud-sidebar tcloud-explorer-sidebar is-collapsed" style={{ width: 52 }}>
                <div className="tcloud-explorer-brand is-collapsed">
                    <img src="/logo.svg" className="h-7 w-7" alt="TCloud" />
                </div>
                <div className="tcloud-explorer-collapsed-nav">
                    <button title="Início" onClick={() => { setActiveFolderId(null); onRootViewChange('dashboard'); }}><Home className="h-4 w-4" /></button>
                    <button title="Abrir TCloud no Explorer" onClick={() => void openCfapiRoot()}><HardDrive className="h-4 w-4" /></button>
                    <button title="Pastas" onClick={() => onOpenForums?.()}><Folder className="h-4 w-4" /></button>
                    <button title="Configurações" onClick={() => onOpenSettings?.()}><Settings className="h-4 w-4" /></button>
                </div>
                <div className="mt-auto flex flex-col items-center gap-1 border-t border-app-border-subtle p-2">
                    <div className={`h-2 w-2 rounded-full ${isConnected ? 'bg-app-success' : 'bg-app-danger'}`} title={isConnected ? 'Tudo conectado' : 'Sem conexão com a nuvem'} />
                    <button className="tcloud-explorer-icon-button" onClick={() => updateSetting('sidebarCollapsed', false)} title="Expandir barra lateral"><ChevronRight className="h-4 w-4" /></button>
                </div>
            </aside>
        );
    }

    return (
        <aside className="tcloud-sidebar tcloud-explorer-sidebar" style={{ width: 232 }} onClick={(event) => event.stopPropagation()}>
            <div className="tcloud-explorer-brand">
                <img src="/logo.svg" className="h-7 w-7" alt="TCloud" />
                <div className="min-w-0 flex-1">
                    <strong>TCloud</strong>
                    <span>Nuvem pessoal</span>
                </div>
                <button className="tcloud-explorer-icon-button" onClick={() => updateSetting('sidebarCollapsed', true)} title={t('common.collapse_sidebar') || 'Recolher barra lateral'}>
                    <ChevronLeft className="h-4 w-4" />
                </button>
            </div>

            <div className="tcloud-explorer-sidebar-scroll">
                <nav className="tcloud-explorer-nav-group" aria-label="Navegação principal">
                    <NavItem icon={Home} label="Início" active={activeFolderId === null && rootView === 'dashboard'} onClick={() => { setActiveFolderId(null); onRootViewChange('dashboard'); }} />
                    <NavItem icon={Clock3} label="Recentes" active={activeFolderId === null && rootView === 'recent'} onClick={() => { setActiveFolderId(null); onRootViewChange('recent'); }} />
                    <NavItem icon={Star} label="Favoritos" active={activeFolderId === null && rootView === 'favorites'} onClick={() => { setActiveFolderId(null); onRootViewChange('favorites'); }} />

                    {(syncingCount > 0 || rootView === 'sync') && <NavItem icon={Activity} label="Sincronização" badge={syncingCount} badgeTone="busy" active={activeFolderId === null && rootView === 'sync'} onClick={() => { setActiveFolderId(null); onRootViewChange('sync'); }} />}
                    {problemCount > 0 && <NavItem icon={CircleAlert} label="Precisa de atenção" badge={problemCount} badgeTone="danger" active={activeFolderId === null && rootView === 'problems'} onClick={() => { setActiveFolderId(null); onRootViewChange('problems'); }} />}
                </nav>

                <section className="tcloud-explorer-section">
                    <div className="tcloud-explorer-section-title"><span>Este computador</span></div>
                    <div className="tcloud-explorer-drive-root">
                        <span className="tcloud-explorer-tree-spacer" aria-hidden="true" />
                        <button type="button" className="tcloud-explorer-tree-main" onClick={() => void openCfapiRoot()} title="Abrir a pasta TCloud no Explorador do Windows">
                            <HardDrive className="h-4 w-4 text-app-accent" />
                            <span className="truncate">TCloud no Windows</span>
                            <i className={`tcloud-explorer-drive-dot ${cfapiStatus?.provider_connected ? 'is-mounted' : ''}`} title={cfapiStatus?.provider_connected ? 'Disponível no Explorer' : 'Reconectando ao Explorer'} />
                        </button>
                        <button type="button" className="tcloud-explorer-tree-action" onClick={() => void openCfapiRoot()} title="Abrir no Explorador do Windows">
                            <ExternalLink className="h-3.5 w-3.5" />
                        </button>
                    </div>
                    {cfapiStatus && !cfapiStatus.provider_connected && (
                        <button type="button" className="tcloud-explorer-empty-row tcloud-explorer-connect-row" onClick={() => onOpenSettings?.()}>
                            <CloudDownload className="h-4 w-4" />
                            <span>Conectar ao Explorador do Windows</span>
                        </button>
                    )}
                    <button
                        type="button"
                        className={`tcloud-explorer-folder-row ${activeFolderId === null && rootView === 'offline' ? 'is-active' : ''}`}
                        onClick={() => {
                            setActiveFolderId(null);
                            onRootViewChange('offline');
                        }}
                        title="Arquivos disponíveis sem internet"
                    >
                        <CloudDownload className="h-4 w-4" />
                        <span>Disponível offline</span>
                    </button>
                </section>

                <section className="tcloud-explorer-section">
                    <div className="tcloud-explorer-section-title">
                        <span>Meu TCloud</span>
                        <button type="button" onClick={() => onOpenForums?.()} title="Organizar pastas"><MoreHorizontal className="h-3.5 w-3.5" /></button>
                    </div>
                    <button
                        type="button"
                        className={`tcloud-explorer-folder-row tcloud-sidebar-cloud-root ${activeFolderId === null && rootView === 'dashboard' && activeForumId === null ? 'is-active' : ''}`}
                        onClick={() => {
                            setActiveFolderId(null);
                            onRootViewChange('dashboard');
                        }}
                        title="Abrir seus arquivos"
                    >
                        <HardDrive className="h-4 w-4 text-app-accent" />
                        <span>Meus arquivos</span>
                    </button>
                    <button
                        type="button"
                        className={`tcloud-explorer-folder-row tcloud-sidebar-special-row ${activeFolderId === null && rootView === 'saved' ? 'is-active' : ''}`}
                        onClick={() => { setActiveFolderId(null); onRootViewChange('saved'); }}
                    >
                        <Folder className="h-4 w-4" />
                        <span>Mensagens Salvas</span>
                    </button>
                    {forumTree.length === 0 ? (
                        <button className="tcloud-explorer-empty-row" type="button" onClick={() => onOpenForums?.()}>
                            <Folder className="h-4 w-4" />
                            <span>Nova pasta</span>
                        </button>
                    ) : forumTree.map((forum) => {
                        const expanded = expandedForums[forum.id] ?? false;
                        return (
                            <div key={forum.id} className="tcloud-explorer-forum-block">
                                <div
                                    className="tcloud-explorer-drive-root"
                                    onContextMenu={(event) => {
                                        event.preventDefault();
                                        showMainFolderMenuAt(forum.id, forum.name, event.clientX, event.clientY);
                                    }}
                                >
                                    <button type="button" className="tcloud-explorer-tree-toggle" onClick={() => setExpandedForums((current) => ({ ...current, [forum.id]: !expanded }))}>
                                        <ChevronRight className={`h-3.5 w-3.5 transition-transform ${expanded ? 'rotate-90' : ''}`} />
                                    </button>
                                    {sidebarRename?.kind === 'main' && sidebarRename.id === forum.id ? (
                                        <div className="tcloud-explorer-tree-main is-editing">
                                            <Folder className="h-4 w-4 text-app-accent" />
                                            <input
                                                autoFocus
                                                value={sidebarRename.value}
                                                disabled={sidebarRenameBusy}
                                                onChange={(event) => setSidebarRename({ ...sidebarRename, value: event.target.value })}
                                                onKeyDown={(event) => {
                                                    if (event.key === 'Enter') event.currentTarget.blur();
                                                    if (event.key === 'Escape') { setSidebarRename(null); event.currentTarget.blur(); }
                                                }}
                                                onBlur={() => void submitSidebarRename()}
                                            />
                                        </div>
                                    ) : (
                                        <button type="button" className={`tcloud-explorer-tree-main ${activeForumId === forum.id && activeFolderId === null ? 'is-active' : ''}`} onClick={() => onOpenForum?.(forum)} title={`Abrir ${forum.name}`}>
                                            <Folder className="h-4 w-4 text-app-accent" />
                                            <span className="truncate">{forum.name}</span>
                                        </button>
                                    )}
                                    <button
                                        type="button"
                                        className="tcloud-explorer-tree-action"
                                        onClick={(event) => {
                                            event.stopPropagation();
                                            const rect = event.currentTarget.getBoundingClientRect();
                                            showMainFolderMenuAt(forum.id, forum.name, rect.right - 4, rect.bottom + 4);
                                        }}
                                        title={`Opções de ${forum.name}`}
                                    >
                                        <MoreVertical className="h-3.5 w-3.5" />
                                    </button>
                                </div>
                                <div className={`tcloud-explorer-tree-children ${expanded ? 'is-expanded' : ''}`}>
                                    <div className="tcloud-explorer-tree-children-inner">
                                        {forum.topics.length === 0 ? (
                                            <div className="tcloud-explorer-empty-row is-indented" aria-hidden="true"><span>Nenhuma subpasta</span></div>
                                        ) : forum.topics.map((topic) => (
                                            <div
                                                key={topic.id}
                                                className="tcloud-explorer-folder-wrap"
                                                onContextMenu={(event) => {
                                                    event.preventDefault();
                                                    showTopicMenuAt(forum.id, topic.id, topic.name, event.clientX, event.clientY);
                                                }}
                                            >
                                                {sidebarRename?.kind === 'folder' && sidebarRename.id === topic.id ? (
                                                    <div className="tcloud-explorer-folder-row is-editing">
                                                        <Folder className="h-4 w-4" />
                                                        <input
                                                            autoFocus
                                                            value={sidebarRename.value}
                                                            disabled={sidebarRenameBusy}
                                                            onChange={(event) => setSidebarRename({ ...sidebarRename, value: event.target.value })}
                                                            onKeyDown={(event) => {
                                                                if (event.key === 'Enter') void submitSidebarRename();
                                                                if (event.key === 'Escape') setSidebarRename(null);
                                                            }}
                                                            onBlur={() => void submitSidebarRename()}
                                                        />
                                                    </div>
                                                ) : (
                                                    <button
                                                        type="button"
                                                        className={`tcloud-explorer-folder-row ${activeFolderId === topic.id ? 'is-active' : ''}`}
                                                        onPointerEnter={() => onWarmFolder?.(topic.id)}
                                                        onFocus={() => onWarmFolder?.(topic.id)}
                                                        onClick={() => { setActiveFolderId(topic.id); onRootViewChange('dashboard'); }}
                                                    >
                                                        <Folder className="h-4 w-4" />
                                                        <span>{topic.name}</span>
                                                    </button>
                                                )}
                                                <button
                                                    type="button"
                                                    className="tcloud-explorer-row-more"
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        const rect = event.currentTarget.getBoundingClientRect();
                                                        showTopicMenuAt(forum.id, topic.id, topic.name, rect.right - 4, rect.bottom + 4);
                                                    }}
                                                    title={`Opções de ${topic.name}`}
                                                >
                                                    <MoreVertical className="h-3.5 w-3.5" />
                                                </button>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            </div>
                        );
                    })}
                </section>

                {otherFolders.length > 0 && (
                    <section className="tcloud-explorer-section">
                        <div className="tcloud-explorer-section-title"><span>Outros locais</span></div>
                        {otherFolders.map((folder) => (
                            <div
                                key={folder.id}
                                className="tcloud-explorer-folder-wrap"
                                onContextMenu={(event) => {
                                    event.preventDefault();
                                    showDriveMenuAt(folder.id, folder.name, event.clientX, event.clientY);
                                }}
                            >
                                <button
                                    type="button"
                                    className={`tcloud-explorer-folder-row ${activeFolderId === folder.id ? 'is-active' : ''}`}
                                    onPointerEnter={() => onWarmFolder?.(folder.id)}
                                    onFocus={() => onWarmFolder?.(folder.id)}
                                    onClick={() => { setActiveFolderId(folder.id); onRootViewChange('dashboard'); }}
                                >
                                    <DriveFolderIcon name={folder.name} />
                                    <span>{folder.name}</span>
                                </button>
                                <button
                                    type="button"
                                    className="tcloud-explorer-row-more"
                                    onClick={(event) => {
                                        event.stopPropagation();
                                        const rect = event.currentTarget.getBoundingClientRect();
                                        showDriveMenuAt(folder.id, folder.name, rect.right - 4, rect.bottom + 4);
                                    }}
                                    title={`Opções de ${folder.name}`}
                                >
                                    <MoreVertical className="h-3.5 w-3.5" />
                                </button>
                            </div>
                        ))}
                    </section>
                )}

                <section className="tcloud-explorer-section">
                    <div className="tcloud-explorer-section-title"><span>Biblioteca</span></div>
                    <NavItem icon={GalleryVerticalEnd} label="Galeria" active={activeFolderId === null && rootView === 'gallery'} onClick={() => { setActiveFolderId(null); onRootViewChange('gallery'); }} />
                    <NavItem icon={Music2} label="Música" active={activeFolderId === null && rootView === 'music'} onClick={() => { setActiveFolderId(null); onRootViewChange('music'); }} />

                    <NavItem icon={LockKeyhole} label="Cofre" active={activeFolderId === null && rootView === 'vault'} onClick={() => { setActiveFolderId(null); onRootViewChange('vault'); }} />
                </section>
                <section className="tcloud-explorer-section tcloud-sidebar-trash-section">
                    <NavItem
                        icon={Trash2}
                        label="Lixeira"
                        active={activeFolderId === null && rootView === 'trash'}
                        onClick={() => {
                            setActiveFolderId(null);
                            onRootViewChange('trash');
                        }}
                    />
                </section>
            </div>

            <footer className="tcloud-explorer-sidebar-footer">
                <button type="button" className="tcloud-explorer-footer-row" onClick={() => onOpenSettings?.()}>
                    <Settings className="h-4 w-4" />
                    <span>Configurações</span>
                </button>
                <div className="tcloud-explorer-footer-status">
                    <span className={`h-2 w-2 rounded-full ${isConnected ? 'bg-app-success' : 'bg-app-danger'}`} />
                    <span className="min-w-0 flex-1 truncate">{isConnected ? (syncingCount > 0 ? 'Atualizando arquivos' : 'Tudo atualizado') : 'Sem conexão com a nuvem'}</span>
                    <div className="relative">
                        <button
                            type="button"
                            className="tcloud-explorer-icon-button"
                            onMouseDown={(event) => event.stopPropagation()}
                            onClick={(event) => { event.stopPropagation(); setFooterMenuOpen((value) => !value); }}
                            title="Mais opções"
                        >
                            <MoreHorizontal className="h-4 w-4" />
                        </button>
                        {footerMenuOpen && (
                            <div className="tcloud-explorer-footer-menu" onMouseDown={(event) => event.stopPropagation()}>
                                <button type="button" onClick={() => { setFooterMenuOpen(false); onOpenAdvanced?.(); }}><Wrench className="h-4 w-4" />Opções avançadas</button>
                                <button type="button" className="is-danger" onClick={() => { setFooterMenuOpen(false); onLogout(); }}><LogOut className="h-4 w-4" />Sair da conta</button>
                            </div>
                        )}
                    </div>
                </div>
            </footer>

            {mainFolderMenu && (
                <div
                    className="tcloud-explorer-context-menu"
                    style={{ left: Math.min(mainFolderMenu.x, window.innerWidth - 220), top: Math.min(mainFolderMenu.y, window.innerHeight - 150) }}
                    onMouseDown={(event) => event.stopPropagation()}
                >
                    <button type="button" onClick={() => {
                        const folder = forumTree.find((item) => item.id === mainFolderMenu.forumId);
                        setMainFolderMenu(null);
                        if (folder) onOpenForum?.(folder);
                    }}><Folder className="h-4 w-4" />Abrir</button>
                    <button type="button" onClick={() => startSidebarRename('main', mainFolderMenu.forumId, mainFolderMenu.forumName)}><Pencil className="h-4 w-4" />Renomear</button>
                    <div className="tcloud-explorer-menu-separator" />
                    <button type="button" className="is-danger" onClick={() => void removeMainFolder(mainFolderMenu.forumId, mainFolderMenu.forumName)}><Trash2 className="h-4 w-4" />Excluir pasta</button>
                </div>
            )}

            {topicMenu && (
                <div
                    className="tcloud-explorer-context-menu"
                    style={{ left: Math.min(topicMenu.x, window.innerWidth - 230), top: Math.min(topicMenu.y, window.innerHeight - 190) }}
                    onMouseDown={(event) => event.stopPropagation()}
                >
                    <button type="button" onClick={() => startSidebarRename('folder', topicMenu.topicId, topicMenu.topicName)}><Pencil className="h-4 w-4" />Renomear</button>
                    <button type="button" disabled={forumTree.length < 2} onClick={() => beginTopicTransfer(topicMenu.forumId, topicMenu.topicId, topicMenu.topicName, 'move')}><MoveRight className="h-4 w-4" />Mover para...</button>
                    <button type="button" onClick={() => beginTopicTransfer(topicMenu.forumId, topicMenu.topicId, topicMenu.topicName, 'copy')}><Copy className="h-4 w-4" />Copiar para...</button>
                    <div className="tcloud-explorer-menu-separator" />
                    <button type="button" className="is-danger" onClick={() => { onDelete(topicMenu.topicId, topicMenu.topicName); setTopicMenu(null); }}><Trash2 className="h-4 w-4" />Excluir pasta</button>
                </div>
            )}

            {driveMenu && (
                <div
                    className="tcloud-explorer-context-menu"
                    style={{ left: Math.min(driveMenu.x, window.innerWidth - 220), top: Math.min(driveMenu.y, window.innerHeight - 140) }}
                    onMouseDown={(event) => event.stopPropagation()}
                >
                    <button type="button" onClick={() => startSidebarRename('folder', driveMenu.folderId, driveMenu.folderName)}><Pencil className="h-4 w-4" />Renomear</button>
                    <button type="button" onClick={() => { setActiveFolderId(driveMenu.folderId); onRootViewChange('dashboard'); setDriveMenu(null); }}><Folder className="h-4 w-4" />Abrir no TCloud</button>
                    <div className="tcloud-explorer-menu-separator" />
                    <button type="button" className="is-danger" onClick={() => { onDelete(driveMenu.folderId, driveMenu.folderName); setDriveMenu(null); }}><Trash2 className="h-4 w-4" />Excluir pasta</button>
                </div>
            )}

            {topicTransfer && (
                <div className="tcloud-explorer-dialog-backdrop" onMouseDown={() => !isTransferringTopic && setTopicTransfer(null)}>
                    <div className="tcloud-explorer-small-dialog" onMouseDown={(event) => event.stopPropagation()}>
                        <h3>{topicTransfer.mode === 'move' ? 'Mover pasta' : 'Copiar pasta'}</h3>
                        <p>{topicTransfer.topicName}</p>
                        <label>
                            Destino
                            <select value={transferTargetForumId ?? ''} onChange={(event) => setTransferTargetForumId(Number(event.target.value))}>
                                <option value="" disabled>Selecione</option>
                                {forumTree
                                    .filter((forum) => topicTransfer.mode === 'copy' || forum.id !== topicTransfer.forumId)
                                    .map((forum) => <option key={forum.id} value={forum.id}>{forum.name}</option>)}
                            </select>
                        </label>
                        <div className="tcloud-explorer-dialog-actions">
                            <button type="button" onClick={() => setTopicTransfer(null)} disabled={isTransferringTopic}>Cancelar</button>
                            <button type="button" className="is-primary" onClick={() => void executeTopicTransfer()} disabled={isTransferringTopic || transferTargetForumId === null}>
                                {isTransferringTopic ? 'Processando...' : topicTransfer.mode === 'move' ? 'Mover' : 'Copiar'}
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </aside>
    );
}
