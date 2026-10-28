import { useEffect, useCallback } from 'react';

interface UseKeyboardShortcutsProps {
    onSelectAll: () => void;
    onDelete: () => void;
    onEscape: () => void;
    onSearch: () => void;
    onEnter?: () => void;
    onDownload?: () => void;
    onShare?: () => void;
    onRename?: () => void;
    onCreateFolder?: () => void;
    onGoUp?: () => void;
    onCopy?: () => void;
    onCut?: () => void;
    onPaste?: () => void;
    enabled?: boolean;
}

export function useKeyboardShortcuts({
    onSelectAll,
    onDelete,
    onEscape,
    onSearch,
    onEnter,
    onDownload,
    onShare,
    onRename,
    onCreateFolder,
    onGoUp,
    onCopy,
    onCut,
    onPaste,
    enabled = true
}: UseKeyboardShortcutsProps) {

    const handleKeyDown = useCallback((e: KeyboardEvent) => {
        if (!enabled) return;

        const target = e.target as HTMLElement;
        if (
            target.tagName === 'INPUT' ||
            target.tagName === 'TEXTAREA' ||
            target.tagName === 'SELECT' ||
            target.isContentEditable ||
            target.closest('[contenteditable="true"]') ||
            target.closest('[role="dialog"]')
        ) {
            if (e.key === 'Escape') {
                target.blur();
                onEscape();
            }
            return;
        }

        const isMod = e.metaKey || e.ctrlKey;
        const key = e.key.toLowerCase();

        if (isMod && !e.shiftKey && key === 'a') {
            e.preventDefault();
            onSelectAll();
            return;
        }
        if (isMod && !e.shiftKey && key === 'f') {
            e.preventDefault();
            onSearch();
            return;
        }
        if (isMod && e.shiftKey && key === 'n') {
            e.preventDefault();
            onCreateFolder?.();
            return;
        }
        if (isMod && !e.shiftKey && key === 'c') {
            e.preventDefault();
            onCopy?.();
            return;
        }
        if (isMod && !e.shiftKey && key === 'x') {
            e.preventDefault();
            onCut?.();
            return;
        }
        if (isMod && !e.shiftKey && key === 'v') {
            e.preventDefault();
            onPaste?.();
            return;
        }
        if (e.altKey && e.key === 'ArrowUp') {
            e.preventDefault();
            onGoUp?.();
            return;
        }
        if (e.key === 'Delete') {
            e.preventDefault();
            onDelete();
            return;
        }
        // Backspace navigates up instead of deleting, matching Windows Explorer.
        if (e.key === 'Backspace') {
            e.preventDefault();
            onGoUp?.();
            return;
        }
        if (e.key === 'Escape') {
            e.preventDefault();
            onEscape();
            return;
        }
        if (e.key === 'Enter') {
            e.preventDefault();
            onEnter?.();
            return;
        }
        if (e.key === 'F2') {
            e.preventDefault();
            onRename?.();
            return;
        }
        if (isMod && key === 'd') {
            e.preventDefault();
            onDownload?.();
            return;
        }
        if (isMod && e.shiftKey && key === 's') {
            e.preventDefault();
            onShare?.();
            return;
        }
    }, [enabled, onSelectAll, onDelete, onEscape, onSearch, onEnter, onDownload, onShare, onRename, onCreateFolder, onGoUp, onCopy, onCut, onPaste]);

    useEffect(() => {
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [handleKeyDown]);
}
