import { CircleAlert, FileText, LayoutGrid, List, Minus, Plus, RefreshCw } from 'lucide-react';
import type { ExplorerViewMode } from '../../../context/SettingsContext';

interface ExplorerStatusBarProps {
  itemCount: number;
  selectedCount: number;
  selectedSize?: number;
  isConnected: boolean;
  computerConnected: boolean;
  isSyncing: boolean;
  syncingCount?: number;
  problemCount?: number;
  onStatusClick?: () => void;
  viewMode: ExplorerViewMode;
  onViewModeChange: (mode: ExplorerViewMode) => void;
  cardScale: number;
  onCardScaleChange: (scale: number) => void;
}

const VIEW_LABELS: Record<ExplorerViewMode, string> = {
  'extra-large': 'Ícones extra grandes',
  large: 'Ícones grandes',
  medium: 'Ícones médios',
  small: 'Ícones pequenos',
  list: 'Lista',
  details: 'Detalhes',
  gallery: 'Galeria',
};


function formatStatusBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );

  const value = bytes / Math.pow(1024, index);

  return `${new Intl.NumberFormat('pt-BR', {
    maximumFractionDigits: index === 0 ? 0 : 1,
  }).format(value)} ${units[index]}`;
}
export function ExplorerStatusBar({
  itemCount,
  selectedCount,
  selectedSize = 0,
  isConnected,
  computerConnected,
  isSyncing,
  syncingCount = 0,
  problemCount = 0,
  onStatusClick,
  viewMode,
  onViewModeChange,
  cardScale,
  onCardScaleChange,
}: ExplorerStatusBarProps) {
  const scalable = !['list', 'details'].includes(viewMode);
  const effectiveSyncing = isSyncing || syncingCount > 0;
  const tone = !isConnected || !computerConnected ? 'offline' : problemCount > 0 ? 'danger' : effectiveSyncing ? 'busy' : 'online';
  const statusText = !isConnected
    ? 'Sem conexão com a nuvem'
    : !computerConnected
      ? 'Reconectando ao Windows…'
      : problemCount > 0
        ? 'Precisa de atenção'
        : effectiveSyncing
          ? syncingCount > 0
            ? `Sincronizando ${syncingCount} ${syncingCount === 1 ? 'item' : 'itens'}…`
            : 'Sincronizando…'
          : 'Tudo atualizado';

  return (
    <div className="tcloud-explorer-statusbar">
      <div className="tcloud-explorer-status-left">
        <span>{itemCount} {itemCount === 1 ? 'item' : 'itens'}</span>
        {selectedCount > 0 && <span>{selectedCount} selecionado{selectedCount === 1 ? '' : 's'}</span>}
        {selectedCount > 0 && selectedSize > 0 && (
          <span title="Tamanho dos arquivos selecionados">
            {formatStatusBytes(selectedSize)}
          </span>
        )}
      </div>
      <button
        type="button"
        className={`tcloud-explorer-status-center is-${tone}`}
        onClick={onStatusClick}
        disabled={!onStatusClick}
        title={onStatusClick ? 'Abrir sincronização' : statusText}
      >
        {problemCount > 0 && isConnected && computerConnected
          ? <CircleAlert className="h-3.5 w-3.5" />
          : effectiveSyncing && isConnected && computerConnected
            ? <RefreshCw className="h-3.5 w-3.5 animate-spin" />
            : <span className={`tcloud-explorer-status-dot ${tone === 'online' ? 'is-online' : ''}`} />}
        <span>{statusText}</span>
      </button>
      <div className="tcloud-explorer-status-view">
        <span className="tcloud-explorer-view-label">{VIEW_LABELS[viewMode]}</span>
        <button type="button" className={viewMode === 'details' ? 'is-active' : ''} onClick={() => onViewModeChange('details')} title="Detalhes"><FileText className="h-3.5 w-3.5" /></button>
        <button type="button" className={viewMode === 'list' ? 'is-active' : ''} onClick={() => onViewModeChange('list')} title="Lista"><List className="h-3.5 w-3.5" /></button>
        {scalable && (
          <>
            <button type="button" onClick={() => onCardScaleChange(Math.max(0.65, cardScale - 0.05))} title="Diminuir itens"><Minus className="h-3.5 w-3.5" /></button>
            <input type="range" min="0.65" max="1.65" step="0.05" value={cardScale} onChange={(event) => onCardScaleChange(Number(event.target.value))} aria-label="Tamanho dos itens" />
            <button type="button" onClick={() => onCardScaleChange(Math.min(1.65, cardScale + 0.05))} title="Aumentar itens"><Plus className="h-3.5 w-3.5" /></button>
          </>
        )}
        <button type="button" className={!['list', 'details'].includes(viewMode) ? 'is-active' : ''} onClick={() => onViewModeChange('medium')} title="Ícones"><LayoutGrid className="h-3.5 w-3.5" /></button>
      </div>
    </div>
  );
}
