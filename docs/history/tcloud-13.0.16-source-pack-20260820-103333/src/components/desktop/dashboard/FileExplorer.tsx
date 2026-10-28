import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { ArrowUpDown, ArrowUp, ArrowDown } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import { useSettings, type ExplorerViewMode } from '../../../context/SettingsContext';
import { FileCard } from './FileCard';
import { EmptyState } from './EmptyState';
import { TelegramFile, TelegramFolder } from '../../../types';
import { ContextMenu } from './ContextMenu';
import { ExplorerBackgroundMenu } from './ExplorerBackgroundMenu';
import { FileListItem } from './FileListItem';
import { Skeleton } from '../../ui';

export type SortField = 'name' | 'size' | 'date';
export type SortDirection = 'asc' | 'desc';

const GRID_GAP = 10;

interface FileExplorerProps {
    files: TelegramFile[];
    loading?: boolean;
    isLoading?: boolean;
    error: Error | null;
    viewMode: ExplorerViewMode;
    selectedIds: number[];
    activeFolderId: number | null;
    onFileClick: (e: React.MouseEvent, id: number, orderedFiles: TelegramFile[]) => void;
    onDelete: (id: number) => void;
    onDownload: (id: number, name: string) => void;
    onPreview: (file: TelegramFile, orderedFiles?: TelegramFile[]) => void;
    onManualUpload?: () => void;
    onFolderUpload?: () => void;
    onCreateFolderRequest?: () => void;
    onRefresh?: () => void | Promise<void>;
    onCopySelection?: () => void;
    onCutSelection?: () => void;
    onPaste?: () => void;
    showFolderUpload?: boolean;
    onToggleSelection: (id: number) => void;
    onShare?: (file: TelegramFile) => void;
    onRename?: (file: TelegramFile) => void;
    onFileMove?: (file: TelegramFile) => void;
    onMove?: (file: TelegramFile) => void;
    onDetails?: (file: TelegramFile) => void;
    onKeepOffline?: (file: TelegramFile) => void;
    onFreeSpace?: (file: TelegramFile) => void;
    onVersion?: (file: TelegramFile) => void;
    onBulkDownload?: () => void;
    onBulkDelete?: () => void;
    onBulkShare?: () => void;
    onBulkMove?: () => void;
    onBulkKeepOffline?: () => void;
    onBulkFreeSpace?: () => void;
    folders?: TelegramFolder[];
    cardScale: number;
    sortField: SortField;
    sortDirection: SortDirection;
    onSortChange: (field: SortField) => void;
    setActiveFolderId?: (id: number | null) => void;
}

function useContainerWidth(containerRef: React.RefObject<HTMLDivElement | null>) {
    const [containerWidth, setContainerWidth] = useState(900);

    useEffect(() => {
        const element = containerRef.current;
        if (!element) return;
        let frame = 0;
        let lastWidth = -1;
        const measure = () => {
            frame = 0;
            const cs = getComputedStyle(element);
            const padding = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
            const next = Math.max(320, Math.round(element.clientWidth - padding));
            if (next !== lastWidth) {
                lastWidth = next;
                setContainerWidth(next);
            }
        };
        const update = () => {
            if (frame) return;
            frame = window.requestAnimationFrame(measure);
        };
        measure();
        const observer = new ResizeObserver(update);
        observer.observe(element);
        return () => {
            observer.disconnect();
            if (frame) window.cancelAnimationFrame(frame);
        };
    }, [containerRef]);

    return containerWidth;
}

const MODE_WIDTH: Record<Exclude<ExplorerViewMode, 'list' | 'details'>, number> = {
    'extra-large': 310,
    large: 245,
    medium: 190,
    small: 125,
    gallery: 255,
};

const MODE_MIN_HEIGHT: Record<Exclude<ExplorerViewMode, 'list' | 'details'>, number> = {
    'extra-large': 230,
    large: 185,
    medium: 145,
    small: 92,
    gallery: 205,
};

export function FileExplorer({
    files, loading, isLoading, error, viewMode, selectedIds, activeFolderId,
    onFileClick, onDelete, onDownload, onPreview, onManualUpload, onFolderUpload, onCreateFolderRequest, onRefresh, onCopySelection, onCutSelection, onPaste, onToggleSelection, onShare, onRename, onFileMove, onMove,
    onDetails, onKeepOffline, onFreeSpace, onVersion, onBulkDownload, onBulkDelete, onBulkShare, onBulkMove, onBulkKeepOffline, onBulkFreeSpace, folders, cardScale, sortField, sortDirection, onSortChange, setActiveFolderId
}: FileExplorerProps) {
    const [contextMenu, setContextMenu] = useState<{ x: number; y: number; file: TelegramFile } | null>(null);
    const [backgroundMenu, setBackgroundMenu] = useState<{ x: number; y: number } | null>(null);
    const { t } = useTranslation();
    const { settings } = useSettings();
    const parentRef = useRef<HTMLDivElement>(null);
    const containerWidth = useContainerWidth(parentRef);
    const isGridMode = !['list', 'details'].includes(viewMode);
    const moveHandler = onFileMove ?? onMove;
    const effectiveLoading = loading ?? isLoading ?? false;
    const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds]);
    const nameCollator = useMemo(
        () => new Intl.Collator(settings.language, { numeric: true, sensitivity: 'base' }),
        [settings.language],
    );

    const baseWidth = isGridMode ? MODE_WIDTH[viewMode as Exclude<ExplorerViewMode, 'list' | 'details'>] : 190;
    const targetWidth = Math.max(88, baseWidth * cardScale);
    const columns = isGridMode
        ? Math.max(1, Math.floor((containerWidth + GRID_GAP) / (targetWidth + GRID_GAP)))
        : 1;
    const cardWidth = (containerWidth - (GRID_GAP * (columns - 1))) / columns;
    const modeKey = (isGridMode ? viewMode : 'medium') as Exclude<ExplorerViewMode, 'list' | 'details'>;
    const ratio = viewMode === 'gallery' ? 0.78 : viewMode === 'small' ? 0.72 : 0.76;
    const cardHeight = Math.max(MODE_MIN_HEIGHT[modeKey], cardWidth * ratio);
    const rowHeight = viewMode === 'details' ? 34 : 36;

    const handleContextMenu = useCallback((e: React.MouseEvent, file: TelegramFile) => {
        e.preventDefault();
        e.stopPropagation();
        // Windows Explorer behavior: right-clicking outside the current
        // selection selects only that item; right-clicking inside a selection
        // preserves the whole selection for batch actions.
        if (!selectedSet.has(file.id)) {
            onFileClick(e, file.id, files);
        }
        setBackgroundMenu(null);
        setContextMenu({ x: e.clientX, y: e.clientY, file });
    }, [files, onFileClick, selectedSet]);

    useEffect(() => {
        const element = parentRef.current;
        if (!element) return;
        let frame = 0;
        let pendingX = 0;
        let pendingY = 0;
        let scrollIdleTimer = 0;
        const markScrolling = () => {
            element.classList.add('is-scrolling');
            if (scrollIdleTimer) window.clearTimeout(scrollIdleTimer);
            scrollIdleTimer = window.setTimeout(() => element.classList.remove('is-scrolling'), 140);
        };
        const flushWheel = () => {
            frame = 0;
            if (pendingY) element.scrollTop += pendingY;
            if (pendingX) element.scrollLeft += pendingX;
            pendingX = 0;
            pendingY = 0;
        };
        const onNativeWheel = (event: WheelEvent) => {
            if (event.ctrlKey) return;
            const multiplier = event.deltaMode === WheelEvent.DOM_DELTA_LINE
                ? 32
                : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
                    ? element.clientHeight
                    : 1;
            const deltaY = event.deltaY * multiplier;
            const deltaX = event.deltaX * multiplier;
            const canScrollY = element.scrollHeight > element.clientHeight + 1;
            const canScrollX = element.scrollWidth > element.clientWidth + 1;
            if ((!canScrollY || deltaY === 0) && (!canScrollX || deltaX === 0)) return;
            event.preventDefault();
            markScrolling();
            if (canScrollY) pendingY += deltaY;
            if (canScrollX) pendingX += deltaX;
            if (!frame) frame = window.requestAnimationFrame(flushWheel);
        };
        element.addEventListener('wheel', onNativeWheel, { passive: false, capture: true });
        element.addEventListener('scroll', markScrolling, { passive: true });
        return () => {
            element.removeEventListener('wheel', onNativeWheel, { capture: true } as EventListenerOptions);
            element.removeEventListener('scroll', markScrolling);
            element.classList.remove('is-scrolling');
            if (scrollIdleTimer) window.clearTimeout(scrollIdleTimer);
            if (frame) window.cancelAnimationFrame(frame);
        };
    }, [effectiveLoading, error, files.length, viewMode]);

    const sortedFiles = useMemo(() => {
        return [...files].sort((a, b) => {
            /*
             * Windows Explorer behavior:
             * folders stay together before regular files.
             *
             * The selected sort mode is still respected inside
             * each group, so Nome/Tamanho/Data continue working.
             */
            const aFolder = a.type === 'folder';
            const bFolder = b.type === 'folder';

            if (aFolder !== bFolder) {
                return aFolder ? -1 : 1;
            }

            let comparison = 0;

            switch (sortField) {
                case 'name':
                    comparison = nameCollator.compare(a.name, b.name);
                    break;

                case 'size':
                    comparison = (a.size || 0) - (b.size || 0);
                    break;

                case 'date':
                    comparison = (a.created_at || '').localeCompare(
                        b.created_at || ''
                    );
                    break;
            }

            return sortDirection === 'asc'
                ? comparison
                : -comparison;
        });
    }, [files, sortField, sortDirection, nameCollator]);

    const explorerItemStats = useMemo(() => {
        let folders = 0;
        let regularFiles = 0;

        for (const file of sortedFiles) {
            if (file.type === 'folder') folders++;
            else regularFiles++;
        }

        return {
            folders,
            files: regularFiles,
        };
    }, [sortedFiles]);
    const handlePreviewRequest = useCallback((file: TelegramFile) => {
        onPreview(file, sortedFiles);
    }, [onPreview, sortedFiles]);

    const gridVirtualizer = useVirtualizer({
        count: Math.ceil(sortedFiles.length / columns),
        getScrollElement: () => parentRef.current,
        estimateSize: useCallback(() => cardHeight, [cardHeight]),
        overscan: viewMode === 'small' ? 3 : 2,
        gap: GRID_GAP,
    });

    const listVirtualizer = useVirtualizer({
        count: sortedFiles.length,
        getScrollElement: () => parentRef.current,
        estimateSize: () => rowHeight,
        overscan: 5,
    });

    useEffect(() => {
        if (parentRef.current) parentRef.current.scrollTop = 0;
        gridVirtualizer.scrollToOffset(0);
        listVirtualizer.scrollToOffset(0);
    }, [activeFolderId, viewMode]);

    useEffect(() => {
        gridVirtualizer.measure();
        listVirtualizer.measure();
    }, [columns, cardHeight, rowHeight, viewMode]);

    const SortIcon = ({ field }: { field: SortField }) => {
        if (sortField !== field) return <ArrowUpDown className="h-3 w-3 opacity-40" />;
        return sortDirection === 'asc'
            ? <ArrowUp className="h-3 w-3 text-app-accent" />
            : <ArrowDown className="h-3 w-3 text-app-accent" />;
    };

    if (effectiveLoading) {
        return (
            <div className="custom-scrollbar flex-1 overflow-hidden px-4 py-3" aria-label={t('common.loading')}>
                {isGridMode ? (
                    <div className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2.5">
                        {Array.from({ length: 18 }, (_, index) => (
                            <div key={index} className="h-[132px] overflow-hidden rounded-lg border border-app-border-subtle bg-app-surface/25 p-3">
                                <Skeleton className="mx-auto h-12 w-12" />
                                <Skeleton className="mt-5 h-3 w-4/5" />
                                <Skeleton className="mt-2 h-2.5 w-2/5" />
                            </div>
                        ))}
                    </div>
                ) : (
                    <div className="border-y border-app-border-subtle">
                        {Array.from({ length: 14 }, (_, index) => (
                            <div key={index} className="flex h-9 items-center gap-3 border-b border-app-border-subtle px-3 last:border-b-0">
                                <Skeleton className="h-4 w-4" />
                                <Skeleton className="h-3 w-[min(22rem,55%)]" />
                                <Skeleton className="ms-auto h-3 w-16" />
                            </div>
                        ))}
                    </div>
                )}
            </div>
        );
    }

    if (error) return <div className="flex flex-1 items-center justify-center p-5 text-ui text-app-danger">Erro ao carregar arquivos</div>;

    if (files.length === 0) {
        return (
            <div
                className="relative flex-1 overflow-auto p-5"
                onContextMenu={(event) => {
                    event.preventDefault();
                    setContextMenu(null);
                    setBackgroundMenu({ x: event.clientX, y: event.clientY });
                }}
            >
                <EmptyState onUpload={onManualUpload ?? (() => undefined)} />
                {backgroundMenu && onCreateFolderRequest && (
                    <ExplorerBackgroundMenu
                        x={backgroundMenu.x}
                        y={backgroundMenu.y}
                        onClose={() => setBackgroundMenu(null)}
                        onCreateFolder={onCreateFolderRequest}
                        onUpload={onManualUpload}
                        onFolderUpload={onFolderUpload}
                        onPaste={onPaste}
                        onRefresh={onRefresh}
                    />
                )}
            </div>
        );
    }

    const cardVariant = viewMode === 'gallery' ? 'gallery' : viewMode === 'small' ? 'small' : 'standard';

    return (
        <div
            ref={parentRef}
            className={`tcloud-explorer custom-scrollbar flex-1 overflow-auto px-4 pb-4 pt-3 tcloud-view-${viewMode}`}
            data-view-mode={viewMode}
            onContextMenu={(event) => {
                event.preventDefault();
                setContextMenu(null);
                setBackgroundMenu({ x: event.clientX, y: event.clientY });
            }}
            onClick={(event) => {
                if (event.target === event.currentTarget) {
                    setContextMenu(null);
                    setBackgroundMenu(null);
                }
            }}
        >
            <div className="tcloud-explorer-group-summary">
                {explorerItemStats.folders > 0 && (
                    <span>
                        {explorerItemStats.folders}
                        {' '}
                        {explorerItemStats.folders === 1 ? 'pasta' : 'pastas'}
                    </span>
                )}

                {explorerItemStats.folders > 0 && explorerItemStats.files > 0 && (
                    <i aria-hidden="true" />
                )}

                {explorerItemStats.files > 0 && (
                    <span>
                        {explorerItemStats.files}
                        {' '}
                        {explorerItemStats.files === 1 ? 'arquivo' : 'arquivos'}
                    </span>
                )}
            </div>

            {isGridMode ? (
                <div className="relative w-full" style={{ height: `${gridVirtualizer.getTotalSize()}px` }}>
                    {gridVirtualizer.getVirtualItems().map((virtualRow) => {
                        const rowStart = virtualRow.index * columns;
                        const row = sortedFiles.slice(rowStart, rowStart + columns);
                        return (
                            <div
                                key={virtualRow.key}
                                className="absolute left-0 top-0 grid w-full"
                                style={{
                                    height: `${cardHeight}px`,
                                    transform: `translateY(${virtualRow.start}px)`,
                                    gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                                    gap: `${GRID_GAP}px`,
                                }}
                            >
                                {row.map((file) => (
                                    <FileCard
                                        key={file.id}
                                        file={file}
                                        isSelected={selectedSet.has(file.id)}
                                        onClick={(e) => onFileClick(e, file.id, sortedFiles)}
                                        onContextMenu={(e) => handleContextMenu(e, file)}
                                        onOpen={() => file.type === 'folder' ? setActiveFolderId?.(file.id) : handlePreviewRequest(file)}
                                        onDelete={() => onDelete(file.id)}
                                        onDownload={() => onDownload(file.id, file.name)}
                                        onPreview={() => handlePreviewRequest(file)}
                                        activeFolderId={activeFolderId}
                                        height={cardHeight}
                                        onToggleSelection={() => onToggleSelection(file.id)}
                                        onShare={onShare ? () => onShare(file) : undefined}
                                        selectedIds={selectedSet.has(file.id) ? selectedIds : undefined}
                                        variant={cardVariant}
                                    />
                                ))}
                            </div>
                        );
                    })}
                </div>
            ) : (
                <div className={`tcloud-list-shell flex w-full flex-col overflow-hidden ${viewMode === 'details' ? 'is-details' : 'is-list'}`}>
                    {viewMode === 'details' && (
                        <div className="tcloud-details-header select-none">
                            <div />
                            <button onClick={() => onSortChange('name')} className="flex items-center gap-1">Nome <SortIcon field="name" /></button>
                            <button onClick={() => onSortChange('date')} className="flex items-center gap-1">Modificado <SortIcon field="date" /></button>
                            <span>Tipo</span>
                            <button onClick={() => onSortChange('size')} className="flex items-center justify-end gap-1">Tamanho <SortIcon field="size" /></button>
                            <span>Status</span>
                            <div />
                        </div>
                    )}
                    <div className="relative w-full" style={{ height: `${listVirtualizer.getTotalSize()}px` }}>
                        {listVirtualizer.getVirtualItems().map((virtualItem) => {
                            const file = sortedFiles[virtualItem.index];
                            return (
                                <div key={file.id} className="absolute left-0 top-0 w-full" style={{ transform: `translateY(${virtualItem.start}px)` }}>
                                    <FileListItem
                                        file={file}
                                        isSelected={selectedSet.has(file.id)}
                                        dragFileIds={selectedSet.has(file.id) ? selectedIds : undefined}
                                        onFileClick={(e, id) => onFileClick(e, id, sortedFiles)}
                                        handleContextMenu={handleContextMenu}
                                        onOpen={() => file.type === 'folder' ? setActiveFolderId?.(file.id) : handlePreviewRequest(file)}
                                        variant={viewMode === 'details' ? 'details' : 'list'}
                                    />
                                </div>
                            );
                        })}
                    </div>
                </div>
            )}

            {backgroundMenu && onCreateFolderRequest && (
                <ExplorerBackgroundMenu
                    x={backgroundMenu.x}
                    y={backgroundMenu.y}
                    onClose={() => setBackgroundMenu(null)}
                    onCreateFolder={onCreateFolderRequest}
                    onUpload={onManualUpload}
                    onFolderUpload={onFolderUpload}
                    onPaste={onPaste}
                    onRefresh={onRefresh}
                />
            )}

            {contextMenu && (
                <ContextMenu
                    x={contextMenu.x}
                    y={contextMenu.y}
                    file={contextMenu.file}
                    onClose={() => setContextMenu(null)}
                    onDownload={() => { onDownload(contextMenu.file.id, contextMenu.file.name); setContextMenu(null); }}
                    onDelete={() => { onDelete(contextMenu.file.id); setContextMenu(null); }}
                    onPreview={() => {
                        if (contextMenu.file.type === 'folder') {
                            setActiveFolderId?.(contextMenu.file.id);
                        } else {
                            handlePreviewRequest(contextMenu.file);
                        }
                        setContextMenu(null);
                    }}
                    onShare={onShare ? () => { onShare(contextMenu.file); setContextMenu(null); } : undefined}
                    onRename={onRename ? () => { onRename(contextMenu.file); setContextMenu(null); } : undefined}
                    onMove={moveHandler ? () => { moveHandler(contextMenu.file); setContextMenu(null); } : undefined}
                    onDetails={onDetails ? () => { onDetails(contextMenu.file); setContextMenu(null); } : undefined}
                    onKeepOffline={onKeepOffline ? () => { onKeepOffline(contextMenu.file); setContextMenu(null); } : undefined}
                    onFreeSpace={onFreeSpace ? () => { onFreeSpace(contextMenu.file); setContextMenu(null); } : undefined}
                    onVersion={onVersion ? () => { onVersion(contextMenu.file); setContextMenu(null); } : undefined}
                    onCopy={onCopySelection}
                    onCut={onCutSelection}
                    folders={folders}
                    activeFolderId={activeFolderId}
                    selectionCount={selectedSet.has(contextMenu.file.id) ? selectedIds.length : 1}
                    onBulkDownload={onBulkDownload}
                    onBulkDelete={onBulkDelete}
                    onBulkShare={onBulkShare}
                    onBulkMove={onBulkMove}
                    onBulkKeepOffline={onBulkKeepOffline}
                    onBulkFreeSpace={onBulkFreeSpace}
                />
            )}
        </div>
    );
}
