import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
    Eye, Download, Trash2, FolderOpen, Pencil, Play, FileText, Copy,
    ArrowRightLeft, Star, Info, Pin, Eraser, History, Share2, Files, Scissors,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { TelegramFile, TelegramFolder } from '../../../types';
import { isMediaFile, isPdfFile } from '../../../utils';
import { toast } from 'sonner';
import { isFavorite, toggleFavorite } from '../../../services/tcloudFavorites';

interface ContextMenuProps {
    x: number;
    y: number;
    file: TelegramFile;
    onClose: () => void;
    onDownload: () => void;
    onDelete: () => void;
    onPreview: () => void;
    onShare?: () => void;
    onRename?: () => void;
    onMove?: () => void;
    onDetails?: () => void;
    onKeepOffline?: () => void;
    onFreeSpace?: () => void;
    onVersion?: () => void;
    onCopy?: () => void;
    onCut?: () => void;
    folders?: TelegramFolder[];
    activeFolderId?: number | null;
    selectionCount?: number;
    onBulkDownload?: () => void;
    onBulkDelete?: () => void;
    onBulkShare?: () => void;
    onBulkMove?: () => void;
    onBulkKeepOffline?: () => void;
    onBulkFreeSpace?: () => void;
}

export function ContextMenu({
    x, y, file, onClose, onDownload, onDelete, onPreview, onShare, onRename, onMove,
    onDetails, onKeepOffline, onFreeSpace, onVersion, onCopy, onCut, folders, activeFolderId,
    selectionCount = 1, onBulkDownload, onBulkDelete, onBulkShare, onBulkMove,
    onBulkKeepOffline, onBulkFreeSpace,
}: ContextMenuProps) {
    const [adjustedPos, setAdjustedPos] = useState({ x, y });
    const menuRef = useRef<HTMLDivElement>(null);
    const { t } = useTranslation();
    const isFolder = file.type === 'folder';
    const isMulti = selectionCount > 1;
    const localStatus = (file.local_status || '').toLowerCase();
    const isAvailableOffline = Boolean(file.is_pinned) || ['cached', 'offline', 'local', 'available'].some((value) => localStatus.includes(value));

    useLayoutEffect(() => {
        if (!menuRef.current) return;
        const rect = menuRef.current.getBoundingClientRect();
        let newX = x;
        let newY = y;
        if (x + rect.width > window.innerWidth - 8) newX = Math.max(8, x - rect.width);
        if (y + rect.height > window.innerHeight - 8) newY = Math.max(8, window.innerHeight - rect.height - 8);
        setAdjustedPos({ x: newX, y: newY });
    }, [x, y, selectionCount]);

    useEffect(() => {
        const closeOutside = (event: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(event.target as Node)) onClose();
        };
        const close = () => onClose();
        window.addEventListener('mousedown', closeOutside, true);
        window.addEventListener('contextmenu', closeOutside, true);
        window.addEventListener('resize', close);
        window.addEventListener('blur', close);
        return () => {
            window.removeEventListener('mousedown', closeOutside, true);
            window.removeEventListener('contextmenu', closeOutside, true);
            window.removeEventListener('resize', close);
            window.removeEventListener('blur', close);
        };
    }, [onClose]);

    const run = (action?: () => void) => () => {
        action?.();
        onClose();
    };

    const copyOrigin = async () => {
        const folder = folders?.find(f => f.id === file.folder_id) || folders?.find(f => f.id === activeFolderId);
        const username = folder?.username || (folder as any)?.chat?.username || (folder as any)?.channel?.username;
        if (!username) {
            toast.info('Este arquivo não possui link público de origem.');
            return;
        }
        try {
            await navigator.clipboard.writeText(`https://t.me/${username}/${file.id}`);
            toast.success('Link copiado');
        } catch {
            toast.error(t('notifications.copy_link_failed'));
        }
    };

    const previewLabel = isFolder ? 'Abrir' : isMediaFile(file.name) ? t('common.play') : isPdfFile(file.name) ? t('files.view_pdf') : t('files.preview');
    const PreviewIcon = isFolder ? FolderOpen : isMediaFile(file.name) ? Play : isPdfFile(file.name) ? FileText : Eye;

    if (isMulti) {
        return (
            <div
                ref={menuRef}
                className="tcloud-context-menu is-multi fixed z-[90]"
                style={{ left: adjustedPos.x, top: adjustedPos.y }}
                onClick={(event) => event.stopPropagation()}
                onContextMenu={(event) => event.preventDefault()}
            >
                <div className="tcloud-context-selection-heading"><Files className="h-4 w-4" /><span>{selectionCount} itens selecionados</span></div>
                <div className="tcloud-context-quick-actions">
                    {onBulkDownload && <button type="button" onClick={run(onBulkDownload)}><Download className="h-4 w-4" /><span>Baixar</span></button>}
                    {onBulkMove && <button type="button" onClick={run(onBulkMove)}><ArrowRightLeft className="h-4 w-4" /><span>Mover</span></button>}
                    {onCopy && <button type="button" onClick={run(onCopy)}><Copy className="h-4 w-4" /><span>Copiar</span></button>}
                    {onCut && <button type="button" onClick={run(onCut)}><Scissors className="h-4 w-4" /><span>Recortar</span></button>}
                    {onBulkShare && <button type="button" onClick={run(onBulkShare)}><Share2 className="h-4 w-4" /><span>Compartilhar</span></button>}
                </div>
                <div className="tcloud-context-separator" />
                {onBulkKeepOffline && <button type="button" className="tcloud-context-item" onClick={run(onBulkKeepOffline)}><Pin className="h-4 w-4" /><span>Sempre disponível</span></button>}
                {onBulkFreeSpace && <button type="button" className="tcloud-context-item" onClick={run(onBulkFreeSpace)}><Eraser className="h-4 w-4" /><span>Liberar espaço</span></button>}
                <div className="tcloud-context-separator" />
                {onBulkDelete && <button type="button" className="tcloud-context-item is-danger" onClick={run(onBulkDelete)}><Trash2 className="h-4 w-4" /><span>Mover {selectionCount} itens para a Lixeira</span><kbd>Del</kbd></button>}
            </div>
        );
    }

    return (
        <div
            ref={menuRef}
            className="tcloud-context-menu fixed z-[90]"
            style={{ left: adjustedPos.x, top: adjustedPos.y }}
            onClick={(event) => event.stopPropagation()}
            onContextMenu={(event) => event.preventDefault()}
        >
            <div className="tcloud-context-quick-actions">
                <button type="button" onClick={run(onPreview)} title={previewLabel}><PreviewIcon className="h-4 w-4" /><span>{previewLabel}</span></button>
                {!isFolder && <button type="button" onClick={run(onDownload)} title="Baixar"><Download className="h-4 w-4" /><span>Baixar</span></button>}
                {!isFolder && onShare && <button type="button" onClick={run(onShare)} title="Compartilhar"><Share2 className="h-4 w-4" /><span>Compartilhar</span></button>}
            </div>

            {!isFolder && (
                <>
                    <div className="tcloud-context-separator" />
                    <button type="button" className="tcloud-context-item" onClick={() => {
                        const next = toggleFavorite(file.id, file.folder_id);
                        toast.success(next ? 'Adicionado aos favoritos' : 'Removido dos favoritos');
                        onClose();
                    }}><Star className={`h-4 w-4 ${isFavorite(file.id, file.folder_id) ? 'fill-current text-app-warning' : ''}`} /><span>{isFavorite(file.id, file.folder_id) ? 'Remover dos favoritos' : 'Adicionar aos favoritos'}</span></button>
                    {!isAvailableOffline && onKeepOffline && <button type="button" className="tcloud-context-item" onClick={run(onKeepOffline)}><Pin className="h-4 w-4" /><span>Sempre disponível</span></button>}
                    {isAvailableOffline && onFreeSpace && <button type="button" className="tcloud-context-item" onClick={run(onFreeSpace)}><Eraser className="h-4 w-4" /><span>Liberar espaço</span></button>}
                </>
            )}

            {(onCopy || onCut) && <div className="tcloud-context-separator" />}
            {onCut && <button type="button" className="tcloud-context-item" onClick={run(onCut)}><Scissors className="h-4 w-4" /><span>Recortar</span><kbd>Ctrl+X</kbd></button>}
            {onCopy && <button type="button" className="tcloud-context-item" onClick={run(onCopy)}><Copy className="h-4 w-4" /><span>Copiar</span><kbd>Ctrl+C</kbd></button>}

            {(onMove || onRename || onVersion) && <div className="tcloud-context-separator" />}
            {onMove && <button type="button" className="tcloud-context-item" onClick={run(onMove)}><ArrowRightLeft className="h-4 w-4" /><span>Mover para...</span></button>}
            {onRename && <button type="button" className="tcloud-context-item" onClick={run(onRename)}><Pencil className="h-4 w-4" /><span>Renomear</span><kbd>F2</kbd></button>}
            <div className="tcloud-context-advanced-group">
            {!isFolder && onVersion && <button type="button" className="tcloud-context-item" onClick={run(onVersion)}><History className="h-4 w-4" /><span>Salvar versão atual</span></button>}

            
                    {!isFolder && <button type="button" className="tcloud-context-item" onClick={async () => { await copyOrigin(); onClose(); }}><Copy className="h-4 w-4" /><span>Copiar link</span></button>}
</div>
            {!isFolder && <div className="tcloud-context-separator" />}

            <div className="tcloud-context-separator" />
            <button type="button" className="tcloud-context-item is-danger" onClick={run(onDelete)}><Trash2 className="h-4 w-4" /><span>Mover para a Lixeira</span><kbd>Del</kbd></button>
            {onDetails && <button type="button" className="tcloud-context-item" onClick={run(onDetails)}><Info className="h-4 w-4" /><span>Propriedades</span></button>}
        </div>
    );
}
