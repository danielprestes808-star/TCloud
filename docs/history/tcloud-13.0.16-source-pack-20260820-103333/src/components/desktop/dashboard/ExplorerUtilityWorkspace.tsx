import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  ArchiveRestore,
  CheckCircle2,
  CircleAlert,
  Cloud,
  Loader2,
  LockKeyhole,
  Pause,
  Play,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from 'lucide-react';
import { toast } from 'sonner';
import {
  listTrash,
  restoreTrash,
  restoreTrashMany,
  formatBytesCompact,
  type ProTrashItem,
} from '../../../services/tcloudProductivity';
import { purgeTrash } from '../../../services/tcloudSyncMaturity';
import {
  clearCompletedTCloudJobs,
  getTCloudSyncCenter,
  pauseTCloudSyncJob,
  resumeTCloudSyncJob,
  retryTCloudSyncJob,
  type TCloudSyncActivity,
} from '../../../services/tcloudSyncCenter';
import { getWindowsHelloStatus, unlockWithWindowsHello } from '../../../services/tcloudCompletion';

export type ExplorerUtilityView = 'trash' | 'sync';

interface ExplorerUtilityWorkspaceProps {
  view: ExplorerUtilityView;
  searchTerm?: string;
  onOpenSyncDetails?: () => void;
}

interface VaultStatus {
  exists: boolean;
  is_unlocked: boolean;
  has_recovery: boolean;
}

function formatWhen(epochSeconds: number) {
  if (!epochSeconds) return '—';
  return new Date(epochSeconds * 1000).toLocaleString('pt-BR');
}

function syncLabel(job: TCloudSyncActivity) {
  const status = (job.status || '').toLowerCase();

  if (status === 'failed' || status.includes('error')) {
    return 'Precisa de atenção';
  }

  if (status === 'running') {
    return 'Sincronizando';
  }

  if (status === 'pending' || status === 'queued' || status === 'waiting') {
    return 'Na fila';
  }

  if (status === 'paused' || job.paused) {
    return 'Pausado';
  }

  if (status === 'completed' || status === 'success') {
    return 'Concluído';
  }

  if (status === 'cancelled' || status === 'canceled') {
    return 'Cancelado';
  }

  return status || 'Finalizado';
}

export function ExplorerUtilityWorkspace({ view, searchTerm = '', onOpenSyncDetails }: ExplorerUtilityWorkspaceProps) {
  const queryClient = useQueryClient();
  const normalized = searchTerm.trim().toLocaleLowerCase('pt-BR');

  const trashQuery = useQuery({
    queryKey: ['tcloud-trash-workspace'],
    queryFn: listTrash,
    enabled: view === 'trash',
    staleTime: 2_000,
  });

  const syncQuery = useQuery({
    queryKey: ['tcloud-sync-workspace'],
    queryFn: getTCloudSyncCenter,
    enabled: view === 'sync',
    refetchInterval: (query) => {
      const data = query.state.data;
      if (!data) return 3_000;

      // Durante uma transferência real a interface acompanha o progresso
      // praticamente em tempo real. Quando não há trabalho, reduzimos
      // bastante o polling para não desperdiçar IPC/SQLite.
      if (data.running > 0) return 1_000;
      if (data.pending > 0 || data.failed > 0) return 2_500;
      return 15_000;
    },
    refetchIntervalInBackground: false,
  });

  useEffect(() => {
    if (view !== 'trash') return;
    const refresh = () => void queryClient.invalidateQueries({ queryKey: ['tcloud-trash-workspace'] });
    window.addEventListener('tcloud-trash-changed', refresh);
    return () => window.removeEventListener('tcloud-trash-changed', refresh);
  }, [queryClient, view]);

  if (view === 'trash') {
    const rows = (trashQuery.data ?? []).filter((item) => !normalized || item.name.toLocaleLowerCase('pt-BR').includes(normalized));
    const restoreOne = async (item: ProTrashItem) => {
      await restoreTrash(item.id);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      toast.success(`${item.name} restaurado.`);
    };
    const deleteOne = async (item: ProTrashItem) => {
      if (!window.confirm(`Excluir definitivamente “${item.name}”?`)) return;
      await purgeTrash(item.id);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      toast.success('Arquivo excluído definitivamente.');
    };
    const restoreAll = async () => {
      const all = trashQuery.data ?? [];
      if (!all.length) return;
      const result = await restoreTrashMany(all.map((item) => item.id));
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      toast.success(`${result.restored} item(ns) restaurado(s).`);
      if (result.failed) toast.warning(`${result.failed} item(ns) não puderam ser restaurados.`);
    };
    const empty = async () => {
      const count = trashQuery.data?.length ?? 0;
      if (!count || !window.confirm(`Esvaziar a Lixeira e excluir definitivamente ${count} item(ns)?`)) return;
      await purgeTrash(null);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      toast.success('Lixeira esvaziada.');
    };

    return (
      <section className="tcloud-workspace-view">
        <div className="tcloud-workspace-heading">
          <div className="tcloud-workspace-icon is-danger"><Trash2 className="h-5 w-5" /></div>
          <div><h2>Lixeira</h2><p>Arquivos removidos ficam aqui antes da exclusão definitiva.</p></div>
          <div className="tcloud-workspace-actions">
            <button type="button" onClick={() => void restoreAll()} disabled={!trashQuery.data?.length}><ArchiveRestore className="h-4 w-4" />Restaurar tudo</button>
            <button type="button" className="is-danger" onClick={() => void empty()} disabled={!trashQuery.data?.length}><Trash2 className="h-4 w-4" />Esvaziar</button>
          </div>
        </div>

        {trashQuery.isLoading ? (
          <div className="tcloud-workspace-empty"><Loader2 className="h-5 w-5 animate-spin" /><strong>Carregando Lixeira…</strong></div>
        ) : rows.length === 0 ? (
          <div className="tcloud-workspace-empty"><CheckCircle2 className="h-8 w-8" /><strong>{normalized ? 'Nenhum item encontrado' : 'A Lixeira está vazia'}</strong><span>{normalized ? 'Tente outro termo de pesquisa.' : 'Arquivos excluídos aparecerão aqui.'}</span></div>
        ) : (
          <div className="tcloud-workspace-list" role="list">
            <div className="tcloud-workspace-list-head"><span>Nome</span><span>Tamanho</span><span>Excluído em</span><span>Ações</span></div>
            {rows.map((item) => (
              <div className="tcloud-workspace-list-row" role="listitem" key={item.id}>
                <div className="tcloud-workspace-name"><Trash2 className="h-4 w-4" /><span><strong>{item.name}</strong><small>{item.relative_path || 'TCloud'}</small></span></div>
                <span>{formatBytesCompact(item.size)}</span>
                <span>{formatWhen(item.deleted_at)}</span>
                <div className="tcloud-workspace-row-actions"><button type="button" onClick={() => void restoreOne(item)}><ArchiveRestore className="h-3.5 w-3.5" />Restaurar</button><button type="button" className="is-danger" onClick={() => void deleteOne(item)}>Excluir</button></div>
              </div>
            ))}
          </div>
        )}
      </section>
    );
  }

  const sync = syncQuery.data;

  const backendActiveCount =
    (sync?.running ?? 0) +
    (sync?.pending ?? 0);

  const syncJobs = sync?.recent ?? [];

  const recentActiveCount = syncJobs.filter((job) =>
    ['running', 'pending', 'queued', 'waiting'].includes(
      (job.status || '').toLowerCase()
    )
  ).length;

  /*
   * O resumo SQLite e a lista "recent" podem chegar em ciclos diferentes.
   * Nunca mostramos "Tudo atualizado" enquanto qualquer uma das duas
   * fontes ainda disser que existe trabalho.
   */
  const activeCount = Math.max(
    backendActiveCount,
    recentActiveCount
  );

  const activeJobs = syncJobs.filter((job) =>
    ['running', 'pending', 'paused'].includes(
      (job.status || '').toLowerCase()
    )
  );

  const aggregateSpeed = activeJobs
    .filter((job) => (job.status || '').toLowerCase() === 'running')
    .reduce(
      (total, job) => total + Math.max(0, job.speed_bps || 0),
      0
    );

  const statusWeight = (job: TCloudSyncActivity) => {
    const status = (job.status || '').toLowerCase();

    if (status === 'running') return 0;
    if (status === 'failed') return 1;
    if (status === 'pending') return 2;
    if (status === 'paused') return 3;
    if (status === 'completed') return 4;
    if (status === 'cancelled') return 5;

    return 6;
  };

  const jobs = syncJobs
    .filter((job) => {
      const status = (job.status || '').toLowerCase();

      const matchesSearch =
        !normalized ||
        (job.file_name || '')
          .toLocaleLowerCase('pt-BR')
          .includes(normalized);

      if (!matchesSearch) return false;

      // Enquanto há trabalho real, prioriza somente aquilo que importa
      // agora. Histórico cancelado/concluído não deve parecer fila ativa.
      if (activeCount > 0 || (sync?.failed ?? 0) > 0) {
        return [
          'running',
          'pending',
          'queued',
          'waiting',
          'paused',
          'failed'
        ].includes(status);
      }

      return true;
    })
    .slice()
    .sort((a,b) => {
      const state = statusWeight(a) - statusWeight(b);
      if (state !== 0) return state;

      return (b.updated_at || 0) - (a.updated_at || 0);
    });

  const refreshSync = () =>
    void queryClient.invalidateQueries({
      queryKey: ['tcloud-sync-workspace']
    });

  return (
    <section className="tcloud-workspace-view">
      <div className="tcloud-workspace-heading">
        <div className="tcloud-workspace-icon is-accent"><RefreshCw className={`h-5 w-5 ${activeCount > 0 ? 'animate-spin' : ''}`} /></div>
        <div>
          <h2>Sincronização</h2>
          <p>
            {(sync?.failed ?? 0) > 0
              ? `${sync?.failed ?? 0} ${(sync?.failed ?? 0) === 1 ? 'item precisa' : 'itens precisam'} de atenção.`
              : activeCount > 0
                ? `${activeCount} ${activeCount === 1 ? 'item está sendo sincronizado' : 'itens estão sendo sincronizados'} agora.`
                : 'Tudo atualizado. Novas alterações aparecerão aqui automaticamente.'}
          </p>
        </div>
        <div className="tcloud-workspace-actions"><button type="button" onClick={refreshSync}><RefreshCw className="h-4 w-4" />Atualizar</button>{onOpenSyncDetails && <button type="button" onClick={onOpenSyncDetails}>Detalhes</button>}</div>
      </div>

      <div className="tcloud-workspace-metrics">
        <div>
          <span>Em andamento</span>
          <strong>{activeCount}</strong>
        </div>

        <div className={(sync?.failed ?? 0) > 0 ? 'is-danger' : ''}>
          <span>Precisa de atenção</span>
          <strong>{sync?.failed ?? 0}</strong>
        </div>

        <div>
          <span>Concluídos recentes</span>
          <strong>{sync?.completed ?? 0}</strong>
        </div>

        <div>
          <span>{aggregateSpeed > 0 ? 'Velocidade atual' : 'Atividade recente'}</span>
          <strong>
            {aggregateSpeed > 0
              ? `${formatBytesCompact(aggregateSpeed)}/s`
              : syncJobs.length >= 100
                ? '100+'
                : syncJobs.length}
          </strong>
        </div>
      </div>

      {syncQuery.isLoading ? (
        <div className="tcloud-workspace-empty"><Loader2 className="h-5 w-5 animate-spin" /><strong>Carregando sincronização…</strong></div>
      ) : jobs.length === 0 ? (
        <div className="tcloud-workspace-empty">
          {normalized ? (
            <>
              <Cloud className="h-8 w-8" />
              <strong>Nenhuma atividade encontrada</strong>
              <span>Tente outro termo de pesquisa.</span>
            </>
          ) : activeCount > 0 ? (
            <>
              <Loader2 className="h-8 w-8 animate-spin text-app-accent" />
              <strong>Preparando sincronização</strong>
              <span>
                {activeCount === 1
                  ? '1 item está aguardando processamento.'
                  : `${activeCount} itens estão aguardando processamento.`}
              </span>
            </>
          ) : (
            <>
              <CheckCircle2 className="h-8 w-8 text-app-success" />
              <strong>Tudo atualizado</strong>
              <span>Não há arquivos esperando sincronização.</span>
            </>
          )}
        </div>
      ) : (
        <div className="tcloud-workspace-list" role="list">
          <div className="tcloud-workspace-list-head is-sync"><span>Arquivo</span><span>Status</span><span>Progresso</span><span>Ações</span></div>
          {jobs.map((job) => {
            const progress = job.bytes_total > 0 ? Math.min(100, Math.round((job.bytes_transferred / job.bytes_total) * 100)) : (job.status.toLowerCase().includes('complete') ? 100 : 0);
            const failed = /failed|error/i.test(job.status);
            return (
              <div className="tcloud-workspace-list-row is-sync" role="listitem" key={job.id}>
                <div className="tcloud-workspace-name">
                  {failed
                    ? <CircleAlert className="h-4 w-4 text-app-danger" />
                    : /complete|success/i.test(job.status)
                      ? <CheckCircle2 className="h-4 w-4 text-app-success" />
                      : /cancel/i.test(job.status)
                        ? <Cloud className="h-4 w-4 text-app-text-tertiary" />
                        : <RefreshCw className={`h-4 w-4 ${job.status.toLowerCase() === 'running' ? 'animate-spin' : ''}`} />}
                  <span>
                    <strong>{job.file_name || job.file_path || 'Operação do TCloud'}</strong>
                    <small>
                      {job.operation}
                      {job.speed_bps > 0
                        ? ` · ${formatBytesCompact(job.speed_bps)}/s`
                        : ''}
                    </small>
                  </span>
                </div>
                <span className={failed ? 'text-app-danger' : ''}>{syncLabel(job)}</span>
                <div className="tcloud-workspace-progress"><i><b style={{ width: `${progress}%` }} /></i><small>{progress}%</small></div>
                <div className="tcloud-workspace-row-actions">
                  {failed && <button type="button" onClick={async () => { await retryTCloudSyncJob(job.id); refreshSync(); }}><RefreshCw className="h-3.5 w-3.5" />Tentar novamente</button>}
                  {!failed && (
                    <>
                      {((job.status || '').toLowerCase() === 'paused' || job.paused) && (
                        <button
                          type="button"
                          onClick={async () => {
                            await resumeTCloudSyncJob(job.id);
                            refreshSync();
                          }}
                        >
                          <Play className="h-3.5 w-3.5" />
                          Continuar
                        </button>
                      )}

                      {['running','pending','queued','waiting'].includes(
                        (job.status || '').toLowerCase()
                      ) && !job.paused && (
                        <button
                          type="button"
                          onClick={async () => {
                            await pauseTCloudSyncJob(job.id);
                            refreshSync();
                          }}
                        >
                          <Pause className="h-3.5 w-3.5" />
                          Pausar
                        </button>
                      )}
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
      {(sync?.completed ?? 0) > 0 && <div className="tcloud-workspace-footer"><button type="button" onClick={async () => { await clearCompletedTCloudJobs(); refreshSync(); }}>Limpar concluídos</button></div>}
    </section>
  );
}

interface ExplorerVaultHeaderProps {
  itemCount: number;
}

export function ExplorerVaultHeader({ itemCount }: ExplorerVaultHeaderProps) {
  const queryClient = useQueryClient();
  const [passphrase, setPassphrase] = useState('');
  const [busy, setBusy] = useState(false);
  const vaultQuery = useQuery({
    queryKey: ['tcloud-vault-status'],
    queryFn: () => invoke<VaultStatus>('cmd_get_vault_status'),
    staleTime: 1_000,
  });
  const helloQuery = useQuery({
    queryKey: ['tcloud-windows-hello-status'],
    queryFn: getWindowsHelloStatus,
    staleTime: 5_000,
  });
  const status = vaultQuery.data;

  const run = async (task: () => Promise<unknown>, success?: string) => {
    setBusy(true);
    try {
      await task();
      setPassphrase('');
      await queryClient.invalidateQueries({ queryKey: ['tcloud-vault-status'] });
      if (success) toast.success(success);
    } catch (error) {
      const text = String(error).replace(/^Error:\s*/i, '');
      toast.error(text.includes('VAULT') ? 'Não foi possível acessar o Cofre. Confira a senha e tente novamente.' : text);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className={`tcloud-vault-strip ${status?.is_unlocked ? 'is-unlocked' : 'is-locked'}`}>
      <div className="tcloud-vault-strip-icon"><LockKeyhole className="h-5 w-5" /></div>
      <div className="tcloud-vault-strip-copy">
        <h2>Cofre</h2>
        <p>{vaultQuery.isLoading ? 'Verificando a proteção do Cofre…' : vaultQuery.isError ? 'Não foi possível verificar o Cofre agora.' : !status?.exists ? 'Crie uma senha para proteger arquivos com criptografia do TCloud.' : status.is_unlocked ? `${itemCount} item(ns) protegido(s) · Cofre desbloqueado nesta sessão.` : 'Cofre bloqueado. Desbloqueie para abrir arquivos protegidos.'}</p>
      </div>
      <div className="tcloud-vault-strip-controls">
        {vaultQuery.isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : vaultQuery.isError ? <button type="button" onClick={() => void vaultQuery.refetch()}><RefreshCw className="h-4 w-4" />Tentar novamente</button> : <>
          {!status?.exists || !status.is_unlocked ? <input type="password" value={passphrase} onChange={(event) => setPassphrase(event.target.value)} placeholder={!status?.exists ? 'Crie uma senha (8+ caracteres)' : 'Senha do Cofre'} onKeyDown={(event) => { if (event.key === 'Enter') { if (!status?.exists && passphrase.length >= 8) void run(() => invoke('cmd_create_vault', { passphrase }), 'Cofre criado.'); else if (status?.exists && passphrase) void run(() => invoke('cmd_unlock_vault', { passphrase }), 'Cofre desbloqueado.'); } }} /> : null}
          {!status?.exists ? <button type="button" className="is-primary" disabled={busy || passphrase.length < 8} onClick={() => void run(() => invoke('cmd_create_vault', { passphrase }), 'Cofre criado e desbloqueado.')}><ShieldCheck className="h-4 w-4" />Criar Cofre</button> : status.is_unlocked ? <button type="button" onClick={() => void run(() => invoke('cmd_lock_vault'), 'Cofre bloqueado.')} disabled={busy}><LockKeyhole className="h-4 w-4" />Bloquear</button> : <button type="button" className="is-primary" onClick={() => void run(() => invoke('cmd_unlock_vault', { passphrase }), 'Cofre desbloqueado.')} disabled={busy || !passphrase}><LockKeyhole className="h-4 w-4" />Desbloquear</button>}
          {status?.exists && !status.is_unlocked && helloQuery.data?.enrolled && <button type="button" onClick={() => void run(() => unlockWithWindowsHello(), 'Cofre desbloqueado com Windows Hello.')} disabled={busy}><ShieldCheck className="h-4 w-4" />Windows Hello</button>}
        </>}
      </div>
    </section>
  );
}
