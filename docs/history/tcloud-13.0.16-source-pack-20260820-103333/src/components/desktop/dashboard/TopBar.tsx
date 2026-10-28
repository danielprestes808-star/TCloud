import { Fragment, useEffect, useRef, useState } from 'react';
import {
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowUpDown,
    ChevronRight,
    Download,
    FileText,
    Filter,
    FolderInput,
    FolderOpen,
    FolderPlus,
    Globe,
    HardDrive,
    Image,
    Info,
    LayoutGrid,
    List,
    MoreHorizontal,
    Music,
    Pencil,
    Pin,
    Eraser,
    RefreshCw,
    Settings,
    Share2,
    Trash2,
    UploadCloud,
    Video,
    X,
    Archive,
    Copy,
    ClipboardPaste,
    FolderUp,
    Scissors,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { MenuItem, MenuPanel, SearchField } from '../../ui';
import type { SortDirection, SortField } from './FileExplorer';
import type { ExplorerViewMode } from '../../../context/SettingsContext';

type TCloudFileFilter = 'all' | 'images' | 'videos' | 'audio' | 'documents' | 'archives';
type TCloudDateFilter = 'any' | 'today' | '7days' | '30days';

export interface ExplorerBreadcrumbItem {
    label: string;
    onClick?: () => void;
}

interface TopBarProps {
    currentFolderName: string;
    breadcrumbs?: ExplorerBreadcrumbItem[];
    selectedIds: number[];
    onShowMoveModal: () => void;
    onBulkDownload: () => void;
    onBulkDelete: () => void;
    onBulkShare: () => void;
    onBulkKeepOffline: () => void;
    onBulkFreeSpace: () => void;
    onDownloadFolder: () => void;
    onClearSelection: () => void;
    onDetailsClick: () => void;
    onRenameSelection?: () => void;
    onUploadClick: () => void;
    onFolderUploadClick?: () => void;
    onCreateFolderRequest: () => void;
    onCopySelection?: () => void;
    onCutSelection?: () => void;
    onPaste?: () => void;
    clipboardCount?: number;
    onNavigateRoot: () => void;
    onBack?: () => void;
    onForward?: () => void;
    onUp?: () => void;
    canGoBack?: boolean;
    canGoForward?: boolean;
    canGoUp?: boolean;
    isConnected: boolean;
    isSyncing: boolean;
    onSync: () => void | Promise<void>;
    viewMode: ExplorerViewMode;
    setViewMode: (mode: ExplorerViewMode) => void;
    cardScale: number;
    onCardScaleChange: (scale: number) => void;
    sortField: SortField;
    sortDirection: SortDirection;
    onSortChange: (field: SortField) => void;
    searchTerm: string;
    onSearchChange: (term: string) => void;
    fileFilter: TCloudFileFilter;
    dateFilter: TCloudDateFilter;
    activeFilterCount: number;
    onFileFilterChange: (filter: TCloudFileFilter) => void;
    onDateFilterChange: (filter: TCloudDateFilter) => void;
    onClearFilters: () => void;
    onSettingsClick: () => void;
    onTCloudClick: () => void;
    onTCloudSyncClick: () => void;
    onRemoteUploadClick: () => void;
    onOpenExplorer: () => void | Promise<void>;
    problemCount?: number;
    syncingCount?: number;
    canCreateItems?: boolean;
    canUploadItems?: boolean;
    showFileTools?: boolean;
}

export function TopBar({
    currentFolderName,
    breadcrumbs,
    selectedIds,
    onShowMoveModal,
    onBulkDownload,
    onBulkDelete,
    onBulkShare,
    onBulkKeepOffline,
    onBulkFreeSpace,
    onDownloadFolder,
    onClearSelection,
    onDetailsClick,
    onRenameSelection,
    onUploadClick,
    onFolderUploadClick,
    onCreateFolderRequest,
    onCopySelection,
    onCutSelection,
    onPaste,
    clipboardCount = 0,
    onNavigateRoot,
    onBack,
    onForward,
    onUp,
    canGoBack = false,
    canGoForward = false,
    canGoUp = false,
    isConnected,
    isSyncing,
    onSync,
    viewMode,
    setViewMode,
    cardScale,
    onCardScaleChange,
    sortField,
    sortDirection,
    onSortChange,
    searchTerm,
    onSearchChange,
    fileFilter,
    dateFilter,
    activeFilterCount,
    onFileFilterChange,
    onDateFilterChange,
    onClearFilters,
    onSettingsClick,
    onTCloudClick,
    onTCloudSyncClick,
    onRemoteUploadClick,
    onOpenExplorer,
    problemCount = 0,
    syncingCount = 0,
    canCreateItems = true,
    canUploadItems = true,
    showFileTools = true,
}: TopBarProps) {
    const { t } = useTranslation();
    const [showFilter, setShowFilter] = useState(false);
    const [showSort, setShowSort] = useState(false);
    const [showView, setShowView] = useState(false);
    const [showMore, setShowMore] = useState(false);
    const rootRef = useRef<HTMLElement>(null);
    const hasSelection = selectedIds.length > 0;
    const locationLabel = currentFolderName || 'TCloud';

    useEffect(() => {
        const close = (event: MouseEvent) => {
            if (rootRef.current?.contains(event.target as Node)) return;
            setShowFilter(false);
            setShowSort(false);
            setShowView(false);
            setShowMore(false);
        };
        window.addEventListener('mousedown', close);
        return () => window.removeEventListener('mousedown', close);
    }, []);

    const openOnly = (menu: 'filter' | 'sort' | 'view' | 'more') => {
        setShowFilter(menu === 'filter' ? (value) => !value : false);
        setShowSort(menu === 'sort' ? (value) => !value : false);
        setShowView(menu === 'view' ? (value) => !value : false);
        setShowMore(menu === 'more' ? (value) => !value : false);
    };

    return (
        <header ref={rootRef} className="tcloud-explorer-header" onClick={(event) => event.stopPropagation()}>
            <div className="tcloud-explorer-location-row">
                <div className="tcloud-explorer-location-navigation">
                    <div className="tcloud-explorer-history-buttons" aria-label="Histórico de navegação">
                        <button type="button" onClick={onBack} disabled={!canGoBack} title="Voltar (Alt+←)"><ArrowLeft className="h-4 w-4" /></button>
                        <button type="button" onClick={onForward} disabled={!canGoForward} title="Avançar (Alt+→)"><ArrowRight className="h-4 w-4" /></button>
                        <button type="button" onClick={onUp} disabled={!canGoUp} title="Subir um nível (Alt+↑)"><ArrowUp className="h-4 w-4" /></button>
                    </div>
                    <div className="tcloud-explorer-breadcrumb" aria-label="Local atual">
                        {(breadcrumbs && breadcrumbs.length > 0 ? breadcrumbs : [
                            { label: 'TCloud', onClick: onNavigateRoot },
                            ...(locationLabel !== 'TCloud' ? [{ label: locationLabel }] : []),
                        ]).map((item, index, items) => (
                            <Fragment key={`${item.label}-${index}`}>
                                {index > 0 && <ChevronRight className="h-3.5 w-3.5" />}
                                {item.onClick && index < items.length - 1 ? (
                                    <button type="button" onClick={item.onClick} title={`Ir para ${item.label}`}>{item.label}</button>
                                ) : (
                                    <span title={item.label}>{item.label}</span>
                                )}
                            </Fragment>
                        ))}
                    </div>
                </div>

                <SearchField
                    containerClassName="tcloud-explorer-search"
                    placeholder={`Pesquisar em ${locationLabel}`}
                    value={searchTerm}
                    onChange={(event) => onSearchChange(event.target.value)}
                />

                <button
                    type="button"
                    className={`tcloud-explorer-header-icon ${isConnected ? '' : 'is-muted'}`}
                    onClick={() => void onSync()}
                    title={isSyncing ? 'Sincronizando...' : 'Atualizar'}
                >
                    <RefreshCw className={`h-4 w-4 ${isSyncing ? 'animate-spin' : ''}`} />
                </button>
            </div>

            <div className="tcloud-explorer-command-row">
                {hasSelection ? (
                    <>
                        <button type="button" className="tcloud-explorer-command is-compact" onClick={onClearSelection} title="Limpar seleção"><X className="h-4 w-4" /></button>
                        <span className="tcloud-explorer-selection-label">{t('files.items_selected', { count: selectedIds.length })}</span>
                        {onCutSelection && <button type="button" className="tcloud-explorer-command is-compact" onClick={onCutSelection} title="Recortar (Ctrl+X)"><Scissors className="h-4 w-4" /></button>}
                        {onCopySelection && <button type="button" className="tcloud-explorer-command is-compact" onClick={onCopySelection} title="Copiar (Ctrl+C)"><Copy className="h-4 w-4" /></button>}
                        {clipboardCount > 0 && onPaste && <button type="button" className="tcloud-explorer-command is-compact" onClick={onPaste} title={`Colar ${clipboardCount} item(ns) (Ctrl+V)`}><ClipboardPaste className="h-4 w-4" /></button>}
                        {selectedIds.length === 1 && onRenameSelection && <button type="button" className="tcloud-explorer-command" onClick={onRenameSelection} title="Renomear (F2)"><Pencil className="h-4 w-4" /><span>Renomear</span></button>}
                        <button type="button" className="tcloud-explorer-command" onClick={onShowMoveModal}><FolderInput className="h-4 w-4" /><span>Mover</span></button>
                        <button type="button" className="tcloud-explorer-command" onClick={onBulkShare}><Share2 className="h-4 w-4" /><span>Compartilhar</span></button>
                        <button type="button" className="tcloud-explorer-command is-danger" onClick={onBulkDelete} title="Mover para a Lixeira (Delete)"><Trash2 className="h-4 w-4" /><span>Lixeira</span></button>
                        <div className="relative">
                            <button type="button" className="tcloud-explorer-command is-compact" onClick={() => openOnly('more')} title="Mais ações"><MoreHorizontal className="h-4 w-4" /></button>
                            {showMore && (
                                <MenuPanel className="absolute start-0 top-10 z-50 w-60">
                                    <MenuItem onClick={() => { setShowMore(false); onBulkDownload(); }}><Download className="h-4 w-4" />Baixar uma cópia</MenuItem>
                                    <MenuItem onClick={() => { setShowMore(false); onBulkKeepOffline(); }}><Pin className="h-4 w-4" />Sempre disponível</MenuItem>
                                    <MenuItem onClick={() => { setShowMore(false); onBulkFreeSpace(); }}><Eraser className="h-4 w-4" />Liberar espaço</MenuItem>
                                    {selectedIds.length === 1 && <MenuItem onClick={() => { setShowMore(false); onDetailsClick(); }}><Info className="h-4 w-4" />Propriedades</MenuItem>}
                                </MenuPanel>
                            )}
                        </div>
                    </>
                ) : (
                    <>
                        {canUploadItems && <button type="button" className="tcloud-explorer-command is-primary-action" onClick={onUploadClick} title="Enviar arquivos"><UploadCloud className="h-4 w-4" /><span>Upload</span></button>}
                        {canCreateItems && <button type="button" className="tcloud-explorer-command" onClick={onCreateFolderRequest} title="Nova pasta (Ctrl+Shift+N)"><FolderPlus className="h-4 w-4" /><span>Nova pasta</span></button>}
                        {canCreateItems && clipboardCount > 0 && onPaste && <button type="button" className="tcloud-explorer-command" onClick={onPaste} title="Colar (Ctrl+V)"><ClipboardPaste className="h-4 w-4" /><span>Colar</span></button>}
                    </>
                )}

                <div className="tcloud-explorer-command-spacer" />

                {!hasSelection && (
                    <>
                        {showFileTools && <div className="relative">
                            <button
                                type="button"
                                className={`tcloud-explorer-command is-compact ${activeFilterCount > 0 ? 'is-active' : ''}`}
                                onClick={() => openOnly('filter')}
                                title={activeFilterCount > 0 ? `Filtros ativos: ${activeFilterCount}` : 'Filtrar'}
                            >
                                <Filter className="h-4 w-4" />
                                {activeFilterCount > 0 && <b>{activeFilterCount}</b>}
                            </button>
                            {showFilter && (
                                <MenuPanel className="absolute end-0 top-10 z-50 w-72">
                                    <div className="tcloud-explorer-menu-heading">Tipo</div>
                                    <div className="tcloud-explorer-filter-grid">
                                        {([
                                            ['all', 'Todos', FolderInput],
                                            ['images', 'Imagens', Image],
                                            ['videos', 'Vídeos', Video],
                                            ['audio', 'Áudio', Music],
                                            ['documents', 'Documentos', FileText],
                                            ['archives', 'Compactados', Archive],
                                        ] as const).map(([value, label, Icon]) => (
                                            <button key={value} type="button" className={fileFilter === value ? 'is-selected' : ''} onClick={() => onFileFilterChange(value)}><Icon className="h-4 w-4" />{label}</button>
                                        ))}
                                    </div>
                                    <div className="tcloud-explorer-menu-heading">Data</div>
                                    <div className="tcloud-explorer-filter-list">
                                        {([
                                            ['any', 'Qualquer data'],
                                            ['today', 'Hoje'],
                                            ['7days', 'Últimos 7 dias'],
                                            ['30days', 'Últimos 30 dias'],
                                        ] as const).map(([value, label]) => (
                                            <button key={value} type="button" className={dateFilter === value ? 'is-selected' : ''} onClick={() => onDateFilterChange(value)}>{label}</button>
                                        ))}
                                    </div>
                                    {activeFilterCount > 0 && <button type="button" className="tcloud-explorer-menu-clear" onClick={onClearFilters}>Limpar filtros</button>}
                                </MenuPanel>
                            )}
                        </div>}

                        {showFileTools && <div className="relative">
                            <button type="button" className="tcloud-explorer-command" onClick={() => openOnly('sort')}><ArrowUpDown className="h-4 w-4" /><span>Ordenar</span></button>
                            {showSort && (
                                <MenuPanel className="absolute end-0 top-10 z-50 w-48">
                                    {([
                                        ['name', 'Nome'],
                                        ['size', 'Tamanho'],
                                        ['date', 'Data'],
                                    ] as const).map(([field, label]) => (
                                        <MenuItem key={field} onClick={() => onSortChange(field)}>
                                            <span className="flex-1">{label}</span>
                                            {sortField === field && (sortDirection === 'asc' ? <ArrowUp className="h-3.5 w-3.5" /> : <ArrowDown className="h-3.5 w-3.5" />)}
                                        </MenuItem>
                                    ))}
                                </MenuPanel>
                            )}
                        </div>}

                        {showFileTools && <div className="relative">
                            <button type="button" className="tcloud-explorer-command" onClick={() => openOnly('view')}><LayoutGrid className="h-4 w-4" /><span>Exibir</span></button>
                            {showView && (
                                <MenuPanel className="absolute end-0 top-10 z-50 w-64">
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'extra-large' ? 'is-selected' : ''}`} onClick={() => { setViewMode('extra-large'); onCardScaleChange(1); }}><Image className="h-4 w-4" /><span>Ícones extra grandes</span></button>
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'large' ? 'is-selected' : ''}`} onClick={() => { setViewMode('large'); onCardScaleChange(1); }}><LayoutGrid className="h-4 w-4" /><span>Ícones grandes</span></button>
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'medium' ? 'is-selected' : ''}`} onClick={() => { setViewMode('medium'); onCardScaleChange(1); }}><LayoutGrid className="h-4 w-4" /><span>Ícones médios</span></button>
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'small' ? 'is-selected' : ''}`} onClick={() => { setViewMode('small'); onCardScaleChange(1); }}><LayoutGrid className="h-4 w-4" /><span>Ícones pequenos</span></button>
                                    <div className="my-1 h-px bg-app-border-subtle" />
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'list' ? 'is-selected' : ''}`} onClick={() => setViewMode('list')}><List className="h-4 w-4" /><span>Lista</span></button>
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'details' ? 'is-selected' : ''}`} onClick={() => setViewMode('details')}><FileText className="h-4 w-4" /><span>Detalhes</span></button>
                                    <button type="button" className={`tcloud-explorer-view-option ${viewMode === 'gallery' ? 'is-selected' : ''}`} onClick={() => { setViewMode('gallery'); onCardScaleChange(1); }}><Image className="h-4 w-4" /><span>Galeria</span></button>
                                    {!['list', 'details'].includes(viewMode) && (
                                        <div className="tcloud-explorer-view-scale">
                                            <span>Tamanho dos itens</span>
                                            <input type="range" min="0.65" max="1.65" step="0.05" value={cardScale} onChange={(event) => onCardScaleChange(Number(event.target.value))} />
                                        </div>
                                    )}
                                </MenuPanel>
                            )}
                        </div>}

                        <div className="relative">
                            <button type="button" className="tcloud-explorer-command is-compact" onClick={() => openOnly('more')} title="Mais opções"><MoreHorizontal className="h-4 w-4" /></button>
                            {showMore && (
                                <MenuPanel className="absolute end-0 top-10 z-50 w-56">
                                    <MenuItem onClick={() => { setShowMore(false); void onOpenExplorer(); }}><FolderOpen className="h-4 w-4" />Abrir no Explorador</MenuItem>
                                    {onFolderUploadClick && <MenuItem onClick={() => { setShowMore(false); onFolderUploadClick(); }}><FolderUp className="h-4 w-4" />Enviar uma pasta</MenuItem>}
                                    <MenuItem onClick={() => { setShowMore(false); onRemoteUploadClick(); }}><Globe className="h-4 w-4" />Enviar por link</MenuItem>
                                    <MenuItem onClick={() => { setShowMore(false); onDownloadFolder(); }}><HardDrive className="h-4 w-4" />Baixar pasta</MenuItem>
                                    <MenuItem onClick={() => { setShowMore(false); onTCloudSyncClick(); }}><RefreshCw className={`h-4 w-4 ${(isSyncing || syncingCount > 0) ? 'animate-spin' : ''}`} />{problemCount > 0 ? `Sincronização (${problemCount})` : 'Sincronização'}</MenuItem>
                                    <MenuItem onClick={() => { setShowMore(false); onTCloudClick(); }}><Info className="h-4 w-4" />Status</MenuItem>
                                    <MenuItem onClick={() => { setShowMore(false); onSettingsClick(); }}><Settings className="h-4 w-4" />Configurações</MenuItem>
                                </MenuPanel>
                            )}
                        </div>
                    </>
                )}
            </div>
        </header>
    );
}
