import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  CheckCircle2,
  CircleAlert,
  Cloud,
  Folder,
  FolderOpen,
  HardDrive,
  MonitorUp,
  Palette,
  RefreshCw,
  Settings2,
  Share2,
  ShieldCheck,
  Wrench,
  Power,
  Gauge,
  FileArchive,
}  from 'lucide-react';
import {
  checkCfapiIntegrity,
  connectThisComputer,
  getCfapiStatus,
  getWindowsStartupEnabled,
  openCfapiRoot,
  repairComputerConnection,
  retryFailedCfapiOperations,
  setWindowsStartupEnabled,
  type CfapiStatus,
} from '../../../services/tcloudCompletion';
import { getTCloudWatcherStatus, type TCloudWatcherStatus } from '../../../services/tcloudSyncSettings';
import { getTCloudCacheStatus, type TCloudCacheStatus } from '../../../services/tcloudCacheManager';
import { getSyncMaturitySettings, type SyncMaturitySettings } from '../../../services/tcloudSyncMaturity';
import { formatBytes } from '../../../utils';
import { save } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import { exportDiagnostics } from '../../../services/tcloudProductivity';
import { applyPerformanceProfile, getPerformanceProfile, type TCloudPerformanceProfile } from '../../../services/tcloudPerformance';
import { getDesktopPreferences, runTCloudHealthCheck, setDesktopPreferences, type TCloudDesktopPreferences, type TCloudHealthReport } from '../../../services/tcloudDesktopExperience';

type SimpleTarget = 'tcloud' | 'tcloud-folders' | 'themes' | 'encryption' | 'sharing';

interface SyncSummary {
  pending: number;
  running: number;
  failed: number;
  historical_failed?: number;
}

interface Props {
  onNavigate: (target: SimpleTarget) => void;
  onOpenOperations?: () => void;
  onOpenAdvanced?: () => void;
  showAdvanced: boolean;
}

export function TCloudSimpleSettingsHome({ onNavigate, onOpenOperations, onOpenAdvanced, showAdvanced }: Props) {
  const [cfapi, setCfapi] = useState<CfapiStatus | null>(null);
  const [watcher, setWatcher] = useState<TCloudWatcherStatus | null>(null);
  const [cache, setCache] = useState<TCloudCacheStatus | null>(null);
  const [maturity, setMaturity] = useState<SyncMaturitySettings | null>(null);
  const [sync, setSync] = useState<SyncSummary | null>(null);
  const [startupEnabled, setStartupEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [computerBusy, setComputerBusy] = useState(false);
  const [computerMessage, setComputerMessage] = useState('');
  const [desktopPrefs, setDesktopPrefs] = useState<TCloudDesktopPreferences | null>(null);
  const [performanceProfile, setPerformanceProfile] = useState<TCloudPerformanceProfile>(() => getPerformanceProfile());
  const [health, setHealth] = useState<TCloudHealthReport | null>(null);
  const [healthBusy, setHealthBusy] = useState(false);

  const refresh = useCallback(async (showLoading = false) => {
    if (showLoading) setLoading(true);
    try {
      const [nextCfapi, nextWatcher, nextCache, nextMaturity, nextSync, nextStartup, nextDesktopPrefs] = await Promise.all([
        getCfapiStatus().catch(() => null),
        getTCloudWatcherStatus().catch(() => null),
        getTCloudCacheStatus().catch(() => null),
        getSyncMaturitySettings().catch(() => null),
        invoke<SyncSummary>('tcloud_get_sync_overview').catch(() => null),
        getWindowsStartupEnabled().catch(() => false),
        getDesktopPreferences().catch(() => null),
      ]);
      setCfapi(nextCfapi);
      setWatcher(nextWatcher);
      setCache(nextCache);
      setMaturity(nextMaturity);
      setSync(nextSync);
      setStartupEnabled(nextStartup);
      setDesktopPrefs(nextDesktopPrefs);
    } finally {
      if (showLoading) setLoading(false);
    }
  }, []);

  useEffect(() => {
    let timer: number | null = null;

    const stop = () => {
      if (timer !== null) {
        window.clearInterval(timer);
        timer = null;
      }
    };

    const start = () => {
      stop();
      if (document.visibilityState !== 'visible') return;
      timer = window.setInterval(() => void refresh(), 15_000);
    };

    const handleVisibility = () => {
      if (document.visibilityState === 'visible') {
        void refresh();
        start();
      } else {
        stop();
      }
    };

    void refresh(true);
    start();
    document.addEventListener('visibilitychange', handleVisibility);
    return () => {
      stop();
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  }, [refresh]);

  const computerRegistered = Boolean(cfapi?.registered);
  const computerConnected = Boolean(computerRegistered && cfapi?.provider_connected);
  const explorerReady = Boolean(computerConnected && (cfapi?.native_offline_menu || cfapi?.fallback_menu_installed));

  const overall = useMemo(() => {
    if (cfapi && !cfapi.platform_supported) {
      return { tone: 'danger', title: 'Windows não compatível', detail: 'A integração automática com o Explorer não está disponível neste computador.' };
    }
    if (cfapi?.registered && !cfapi.provider_connected) {
      return { tone: 'busy', title: 'Reconectando ao Explorer', detail: 'O TCloud está recuperando a conexão automaticamente. Seus arquivos locais continuam disponíveis.' };
    }
    if (cfapi && !cfapi.registered) {
      return { tone: 'neutral', title: 'Conecte este computador', detail: 'Uma configuração rápida deixa seus arquivos disponíveis no Explorer.' };
    }
    if ((sync?.failed ?? 0) > 0) {
      const total=sync?.failed ?? 0;
      return { tone: 'danger', title: 'Alguns arquivos precisam de atenção', detail: `${total} ${total === 1 ? 'item precisa' : 'itens precisam'} ser revisado(s). O TCloud pode tentar novamente.` };
    }
    if ((sync?.running ?? 0) > 0 || (sync?.pending ?? 0) > 0) {
      const total = (sync?.running ?? 0) + (sync?.pending ?? 0);
      return { tone: 'busy', title: 'Sincronizando seus arquivos', detail: `${total} ${total === 1 ? 'arquivo está sendo atualizado' : 'arquivos estão sendo atualizados'}.` };
    }
    if (watcher?.running && cfapi?.provider_connected) {
      return { tone: 'ok', title: 'Tudo atualizado', detail: 'Seus arquivos estão prontos no TCloud e no Explorer.' };
    }
    return { tone: 'neutral', title: loading ? 'Verificando...' : 'TCloud pronto', detail: 'O TCloud está verificando seus arquivos.' };
  }, [cfapi, loading, sync, watcher]);

  const cacheText = cache
    ? `${formatBytes(cache.current_bytes)} usados no computador`
    : 'Uso local será mostrado aqui';

  const handleComputerConnection = async () => {
    setComputerBusy(true);
    setComputerMessage('');
    try {
      const next = computerRegistered ? await repairComputerConnection() : await connectThisComputer();
      setCfapi(next);
      setStartupEnabled(await getWindowsStartupEnabled().catch(() => true));
      setComputerMessage(computerRegistered ? 'Conexão verificada e reparada.' : 'Computador conectado. O TCloud cuidará da reconexão automaticamente.');
    } catch (error) {
      setComputerMessage(`Não foi possível concluir: ${String(error)}`);
    } finally {
      setComputerBusy(false);
      void refresh();
    }
  };

  const handleRetryWindowsActions = async () => {
    setComputerBusy(true);
    setComputerMessage('');
    try {
      const next=await retryFailedCfapiOperations();
      setCfapi(next);
      setComputerMessage(next.failed_operations > 0
        ? `${next.failed_operations} ação(ões) ainda precisam de atenção.`
        : 'As ações do Explorer foram colocadas novamente na fila.');
    } catch (error) {
      setComputerMessage(`Não foi possível tentar novamente: ${String(error)}`);
    } finally {
      setComputerBusy(false);
      void refresh();
    }
  };

  const handleVerifyExplorer = async () => {
    setComputerBusy(true);
    setComputerMessage('');
    try {
      const result = await checkCfapiIntegrity(true);
      const next = await getCfapiStatus();
      setCfapi(next);
      if (result.missing_placeholders > 0 || result.errors.length > 0) {
        setComputerMessage(`${result.missing_placeholders} arquivo(s) ainda precisam de revisão. O TCloud continuará tentando sozinho.`);
      } else if (result.repaired > 0) {
        setComputerMessage(`${result.repaired} item(ns) do Explorer foram reparados.`);
      } else {
        setComputerMessage('Integração do Explorer verificada. Nenhum reparo foi necessário.');
      }
    } catch (error) {
      setComputerMessage(`Não foi possível verificar os arquivos: ${String(error)}`);
    } finally {
      setComputerBusy(false);
      void refresh();
    }
  };

  const handleStartupChange = async (enabled: boolean) => {
    setComputerBusy(true);
    setComputerMessage('');
    try {
      const actual = await setWindowsStartupEnabled(enabled);
      setStartupEnabled(actual);
      setComputerMessage(actual
        ? 'O TCloud iniciará automaticamente com o Windows.'
        : 'O início automático foi desativado.');
    } catch (error) {
      setComputerMessage(`Não foi possível alterar o início automático: ${String(error)}`);
    } finally {
      setComputerBusy(false);
    }
  };

  const handleCloseBehaviorChange = async (background: boolean) => {
    const current = desktopPrefs ?? { close_behavior: 'background', close_notice_seen: false, quiet_retries: true, auto_free_space_days: 0 };
    try {
      const next = await setDesktopPreferences(background ? 'background' : 'exit', current.quiet_retries, current.auto_free_space_days);
      setDesktopPrefs(next);
      toast.success(background ? 'O TCloud continuará sincronizando quando a janela for fechada.' : 'Fechar a janela encerrará o TCloud completamente.');
    } catch (error) {
      toast.error(`Não foi possível alterar: ${String(error)}`);
    }
  };

  const handleQuietRetriesChange = async (enabled: boolean) => {
    const current = desktopPrefs ?? { close_behavior: 'background', close_notice_seen: false, quiet_retries: true, auto_free_space_days: 0 };
    try {
      setDesktopPrefs(await setDesktopPreferences(current.close_behavior, enabled, current.auto_free_space_days));
    } catch (error) {
      toast.error(`Não foi possível alterar: ${String(error)}`);
    }
  };

  const handlePerformanceChange = async (profile: TCloudPerformanceProfile) => {
    setPerformanceProfile(profile);
    await applyPerformanceProfile(profile);
    toast.success(profile === 'performance' ? 'Desempenho máximo ativado.' : profile === 'economy' ? 'Modo econômico ativado.' : 'Desempenho equilibrado ativado.');
  };

  const handleHealthCheck = async (repair: boolean) => {
    setHealthBusy(true);
    try {
      if (repair) {
        await retryFailedCfapiOperations().catch(() => null);
        await checkCfapiIntegrity(true).catch(() => null);
      } else {
        await checkCfapiIntegrity(false).catch(() => null);
      }
      const report = await runTCloudHealthCheck(repair);
      setHealth(report);
      toast.success(report.summary);
      if (repair) void refresh();
    } catch (error) {
      toast.error(`Não foi possível verificar o TCloud: ${String(error)}`);
    } finally {
      setHealthBusy(false);
    }
  };

  const handleExportDiagnostics = async () => {
    try {
      const outputPath = await save({
        title: 'Salvar diagnóstico do TCloud',
        defaultPath: `TCloud-diagnostico-${new Date().toISOString().slice(0, 10)}.zip`,
        filters: [{ name: 'Arquivo ZIP', extensions: ['zip'] }],
      });
      if (!outputPath) return;
      await exportDiagnostics(outputPath);
      toast.success('Diagnóstico exportado sem senhas, sessão do Telegram ou conteúdo dos seus arquivos.');
    } catch (error) {
      toast.error(`Não foi possível exportar o diagnóstico: ${String(error)}`);
    }
  };

  return (
    <div className="tcloud-simple-home">
      <section className={`tcloud-simple-hero is-${overall.tone}`}>
        <div className="tcloud-simple-hero-icon">
          {overall.tone === 'ok' ? <CheckCircle2 className="h-6 w-6" /> : overall.tone === 'danger' ? <CircleAlert className="h-6 w-6" /> : <Cloud className="h-6 w-6" />}
        </div>
        <div className="min-w-0 flex-1">
          <h3>{overall.title}</h3>
          <p>{overall.detail}</p>
        </div>
        <button type="button" onClick={() => void refresh(true)} className="tcloud-simple-icon-button" title="Atualizar status">
          <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </section>

      <section className={`tcloud-computer-connect ${computerConnected ? 'is-connected' : ''}`}>
        <div className="tcloud-computer-connect-icon"><MonitorUp className="h-5 w-5" /></div>
        <div className="tcloud-computer-connect-copy">
          <div className="tcloud-computer-connect-title">
            <h3>{computerConnected ? 'Este computador está conectado' : computerRegistered ? 'Reconectando este computador' : 'Usar TCloud neste computador'}</h3>
            <span className={computerConnected ? 'is-ok' : ''}>{computerConnected ? 'Conectado' : computerRegistered ? 'Reconectando' : 'Não conectado'}</span>
          </div>
          <p>{computerConnected
            ? 'Integração com o Windows ativa. O TCloud mantém os arquivos somente na nuvem e baixa o conteúdo quando você abre ou fixa um item.'
            : 'Clique uma vez para preparar a pasta TCloud no Windows, ativar a sincronização e deixar a reconexão automática.'}</p>
          {cfapi?.root_path && <small title={cfapi.root_path}>Pasta: {cfapi.root_path}</small>}
          {computerMessage && <div className={`tcloud-computer-message ${computerMessage.startsWith('Não foi') ? 'is-error' : ''}`}>{computerMessage}</div>}
        </div>
        <div className="tcloud-computer-connect-actions">
          {computerConnected ? (
            <>
              <button type="button" className="tcloud-computer-primary" onClick={() => void openCfapiRoot()}>
                <FolderOpen className="h-4 w-4" /> Abrir no Explorer
              </button>
              <button type="button" className="tcloud-computer-secondary" disabled={computerBusy} onClick={() => void handleComputerConnection()}>
                {computerBusy ? <RefreshCw className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />} Verificar conexão
              </button>
            </>
          ) : (
            <button
              type="button"
              className="tcloud-computer-primary"
              disabled={computerBusy || cfapi?.platform_supported === false}
              onClick={() => void handleComputerConnection()}
            >
              {computerBusy ? <RefreshCw className="h-4 w-4 animate-spin" /> : <MonitorUp className="h-4 w-4" />}
              {computerRegistered ? 'Reconectar' : 'Conectar este computador'}
            </button>
          )}
        </div>
        {computerConnected && (
          <details className="tcloud-computer-details">
            <summary>Detalhes da integração</summary>
            <div className="tcloud-computer-capabilities" aria-label="Recursos automáticos neste computador">
              <span><CheckCircle2 className="h-3.5 w-3.5" /> Reconexão automática</span>
              <span><CheckCircle2 className="h-3.5 w-3.5" /> Retoma após reinício</span>
              <span><CheckCircle2 className="h-3.5 w-3.5" /> Arquivos sob demanda · sem download em massa</span>
              <span><CheckCircle2 className="h-3.5 w-3.5" /> {cfapi?.native_offline_menu ? 'Menu nativo do Windows' : 'Menu compatível de offline'}</span>
              <button type="button" className="tcloud-computer-secondary" disabled={computerBusy} onClick={() => void handleVerifyExplorer()}>
                <ShieldCheck className="h-4 w-4" /> Verificar arquivos
              </button>
            </div>
          </details>
        )}
        <label className="tcloud-computer-startup">
          <span><strong>Iniciar com o Windows</strong><small>Recomendado para seus arquivos estarem sempre prontos.</small></span>
          <input
            type="checkbox"
            checked={startupEnabled}
            disabled={computerBusy || cfapi?.platform_supported === false}
            onChange={(event) => void handleStartupChange(event.target.checked)}
          />
        </label>
        {computerConnected && cfapi && (
          <div className="tcloud-computer-availability-summary">
            <span><strong>{cfapi.cloud_only_count}</strong> somente na nuvem</span>
            <span><strong>{cfapi.cached_count}</strong> neste computador</span>
            <span><strong>{cfapi.offline_pinned_count}</strong> sempre offline</span>
          </div>
        )}
        {computerConnected && !explorerReady && <div className="tcloud-computer-warning">O Explorer ainda está finalizando a conexão. O TCloud continuará tentando sozinho; use “Verificar conexão” apenas se quiser conferir agora.</div>}
        {(cfapi?.failed_operations ?? 0) > 0 && (
          <div className="tcloud-computer-warning">
            {cfapi?.failed_operations} ação(ões) do Explorer não puderam ser concluídas automaticamente.
            <button type="button" disabled={computerBusy} onClick={() => void handleRetryWindowsActions()}>Tentar novamente</button>
          </div>
        )}
      </section>

      <div className="tcloud-simple-status-grid">
        <button type="button" onClick={() => onOpenOperations?.()}>
          <RefreshCw className="h-5 w-5" />
          <span><strong>Sincronização</strong><small>{(sync?.failed ?? 0) > 0 ? `${sync?.failed ?? 0} precisam de atenção` : ((sync?.running ?? 0) + (sync?.pending ?? 0)) > 0 ? `${(sync?.running ?? 0) + (sync?.pending ?? 0)} em andamento` : 'Tudo em dia'}</small></span>
        </button>
        <button type="button" onClick={() => onNavigate('tcloud')}>
          <HardDrive className="h-5 w-5" />
          <span><strong>Espaço no computador</strong><small>{cacheText}</small></span>
        </button>
        <button type="button" onClick={() => onNavigate('encryption')}>
          <ShieldCheck className="h-5 w-5" />
          <span><strong>Proteção</strong><small>{maturity ? `${maturity.version_retention} versões · Lixeira ${maturity.trash_retention_days} dias` : 'Versões, lixeira e privacidade'}</small></span>
        </button>
      </div>

      <section className="tcloud-simple-section">
        <div className="tcloud-simple-section-heading">
          <div><h3>O que você quer fazer?</h3><p>As opções mais usadas ficam aqui.</p></div>
        </div>
        <div className="tcloud-simple-action-grid">
          <button type="button" onClick={() => onNavigate('tcloud-folders')}><Folder className="h-5 w-5" /><span><strong>Organizar pastas</strong><small>Escolha onde seus arquivos ficam</small></span></button>
          <button type="button" onClick={() => onNavigate('themes')}><Palette className="h-5 w-5" /><span><strong>Mudar aparência</strong><small>Tema, cores e visual</small></span></button>
          <button type="button" onClick={() => onNavigate('sharing')}><Share2 className="h-5 w-5" /><span><strong>Compartilhar</strong><small>Links e acessos compartilhados</small></span></button>
        </div>
      </section>

      <section className="tcloud-simple-section">
        <div className="tcloud-simple-section-heading">
          <div><h3>Sincronização e desempenho</h3><p>Escolha entre economia, equilíbrio ou maior velocidade.</p></div>
        </div>
        <div className="tcloud-simple-preference-grid">
          <label className="tcloud-switch-row">
            <input
              type="checkbox"
              checked={(desktopPrefs?.close_behavior ?? 'background') === 'background'}
              onChange={(event) => void handleCloseBehaviorChange(event.target.checked)}
            />
            <span><strong>Continuar sincronizando ao fechar a janela</strong><small>O X esconde a janela e mantém o TCloud perto do relógio. Use “Encerrar TCloud” na bandeja para sair completamente.</small></span>
          </label>
          <label className="tcloud-switch-row">
            <input
              type="checkbox"
              checked={desktopPrefs?.quiet_retries ?? true}
              onChange={(event) => void handleQuietRetriesChange(event.target.checked)}
            />
            <span><strong>Resolver tentativas temporárias automaticamente</strong><small>Falhas de internet e novas tentativas não aparecem como problema até realmente precisarem de você.</small></span>
          </label>
          <label className="tcloud-simple-select-row">
            <HardDrive className="h-4 w-4" />
            <span><strong>Economizar espaço automaticamente</strong><small>Nunca remove itens marcados como “Sempre manter neste dispositivo”.</small></span>
            <select
              value={desktopPrefs?.auto_free_space_days ?? 0}
              onChange={async (event) => {
                const current = desktopPrefs ?? { close_behavior: 'background', close_notice_seen: false, quiet_retries: true, auto_free_space_days: 0 };
                try { setDesktopPrefs(await setDesktopPreferences(current.close_behavior, current.quiet_retries, Number(event.target.value))); }
                catch (error) { toast.error(`Não foi possível alterar: ${String(error)}`); }
              }}
            >
              <option value={0}>Desativado</option>
              <option value={7}>Após 7 dias sem uso</option>
              <option value={30}>Após 30 dias sem uso</option>
              <option value={90}>Após 90 dias sem uso</option>
            </select>
          </label>
        </div>
        <div className="tcloud-performance-choice" role="group" aria-label="Perfil de desempenho">
          {([
            ['economy', 'Economia', 'Menos rede e bateria'],
            ['balanced', 'Equilibrado', 'Recomendado'],
            ['performance', 'Desempenho', 'Uploads e miniaturas mais rápidos'],
          ] as const).map(([profile, label, detail]) => (
            <button key={profile} type="button" className={performanceProfile === profile ? 'is-active' : ''} onClick={() => void handlePerformanceChange(profile)}>
              <Gauge className="h-4 w-4" /><span><strong>{label}</strong><small>{detail}</small></span>
            </button>
          ))}
        </div>
      </section>

      <section className="tcloud-simple-section">
        <div className="tcloud-simple-section-heading">
          <div><h3>Verificação e suporte</h3><p>O TCloud pode limpar histórico antigo, verificar o banco e gerar um pacote seguro para diagnóstico.</p></div>
        </div>
        <div className="tcloud-simple-action-grid">
          <button type="button" disabled={healthBusy} onClick={() => void handleHealthCheck(false)}><Power className="h-5 w-5" /><span><strong>Verificar TCloud</strong><small>{health?.summary ?? 'Checar saúde sem alterar seus arquivos'}</small></span></button>
          <button type="button" disabled={healthBusy} onClick={() => void handleHealthCheck(true)}><Wrench className="h-5 w-5" /><span><strong>Corrigir automaticamente</strong><small>{health?.stale_history_cleaned ? `${health.stale_history_cleaned} registro(s) antigo(s) limpos` : 'Limpar histórico resolvido e otimizar o banco'}</small></span></button>
          <button type="button" onClick={() => void handleExportDiagnostics()}><FileArchive className="h-5 w-5" /><span><strong>Exportar diagnóstico</strong><small>ZIP sem senhas, sessão ou conteúdo pessoal</small></span></button>
        </div>
        {health && (health.previous_unclean_shutdown || health.stalled_transfers > 0 || health.failed_cfapi_operations > 0) && (
          <div className="tcloud-computer-warning">
            {health.previous_unclean_shutdown && <span>O início anterior foi interrompido; a recuperação automática verificou o banco.</span>}
            {health.stalled_transfers > 0 && <span>{health.stalled_transfers} transferência(ões) sem progresso há mais de 6 horas.</span>}
            {health.failed_cfapi_operations > 0 && <span>{health.failed_cfapi_operations} ação(ões) do Explorer ainda precisam ser repetidas.</span>}
          </div>
        )}
      </section>

      <section className="tcloud-simple-help-strip">
        <Settings2 className="h-4 w-4" />
        <div><strong>As configurações técnicas ficam escondidas por padrão.</strong><span>Você pode ativá-las quando precisar diagnosticar ou ajustar algo específico.</span></div>
        {showAdvanced && onOpenAdvanced && <button type="button" onClick={onOpenAdvanced}><Wrench className="h-4 w-4" /> Ferramentas avançadas</button>}
      </section>
    </div>
  );
}
