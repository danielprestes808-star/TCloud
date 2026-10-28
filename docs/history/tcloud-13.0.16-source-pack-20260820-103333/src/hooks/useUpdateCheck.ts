import { useState, useEffect, useCallback, useRef } from 'react';
import { check, Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { prepareRollback, cancelRollback } from '../services/tcloudCompletion';

function updaterUnavailableMessage(error: unknown): string | null {
    const raw = error instanceof Error ? error.message : String(error ?? '');
    const normalized = raw.toLowerCase();

    if (
        normalized.includes('endpoint') ||
        normalized.includes('pubkey') ||
        normalized.includes('public key') ||
        normalized.includes('not configured') ||
        normalized.includes('no current version')
    ) {
        return 'Atualizações automáticas ainda não estão configuradas para este build.';
    }

    return null;
}

interface UpdateState {
    checking: boolean;
    available: boolean;
    downloading: boolean;
    progress: number;
    error: string | null;
    version: string | null;
}

export function useUpdateCheck() {
    const [state, setState] = useState<UpdateState>({
        checking: false,
        available: false,
        downloading: false,
        progress: 0,
        error: null,
        version: null,
    });

    const [update, setUpdate] = useState<Update | null>(null);

    const checkingRef = useRef(false);
    const downloadingRef = useRef(false);
    const lastCheckRef = useRef(0);

    const checkForUpdates = useCallback(async () => {
        if (checkingRef.current || downloadingRef.current) {
            return;
        }

        checkingRef.current = true;
        lastCheckRef.current = Date.now();

        setState(s => ({
            ...s,
            checking: true,
            error: null,
        }));

        try {
            const updateInfo = await check();

            if (updateInfo) {
                setUpdate(updateInfo);

                setState(s => ({
                    ...s,
                    checking: false,
                    available: true,
                    version: updateInfo.version,
                    error: null,
                }));
            } else {
                setUpdate(null);

                setState(s => ({
                    ...s,
                    checking: false,
                    available: false,
                    version: null,
                    error: null,
                }));
            }
        } catch (err: unknown) {
            const message =
                updaterUnavailableMessage(err) ??
                (err instanceof Error
                    ? err.message
                    : 'Não foi possível verificar atualizações.');

            setState(s => ({
                ...s,
                checking: false,
                error: message,
            }));
        } finally {
            checkingRef.current = false;
        }
    }, []);

    const downloadAndInstall = useCallback(async () => {
        if (!update || downloadingRef.current) {
            return;
        }

        downloadingRef.current = true;

        setState(s => ({
            ...s,
            downloading: true,
            progress: 0,
            error: null,
        }));

        let downloaded = 0;
        let contentLength = 0;

        try {
            await prepareRollback(update.version);

            await update.downloadAndInstall((event) => {
                if (event.event === 'Started') {
                    const data = event.data as {
                        contentLength?: number;
                    };

                    contentLength = data.contentLength || 0;
                } else if (event.event === 'Progress') {
                    const data = event.data as {
                        chunkLength?: number;
                    };

                    downloaded += data.chunkLength || 0;

                    if (contentLength > 0) {
                        const pct = Math.round(
                            (downloaded / contentLength) * 100
                        );

                        setState(s => ({
                            ...s,
                            progress: Math.min(pct, 100),
                        }));
                    }
                } else if (event.event === 'Finished') {
                    setState(s => ({
                        ...s,
                        progress: 100,
                    }));
                }
            });

            await relaunch();
        } catch (err: unknown) {
            await cancelRollback().catch(() => {});

            const message =
                err instanceof Error
                    ? err.message
                    : 'Não foi possível instalar a atualização.';

            setState(s => ({
                ...s,
                downloading: false,
                error: message,
            }));

            downloadingRef.current = false;
        }
    }, [update]);

    const dismissUpdate = useCallback(() => {
        setState(s => ({
            ...s,
            available: false,
        }));

        setUpdate(null);
    }, []);

    useEffect(() => {
        let disposed = false;

        const runCheck = () => {
            if (disposed) return;

            if (document.visibilityState !== 'visible') {
                return;
            }

            void checkForUpdates();
        };

        // Primeira verificação pouco depois da inicialização.
        const startupTimer = window.setTimeout(runCheck, 5_000);

        // Nova verificação periódica.
        const interval = window.setInterval(() => {
            if (document.visibilityState !== 'visible') {
                return;
            }

            // Evita verificações duplicadas muito próximas.
            if (Date.now() - lastCheckRef.current < 10 * 60_000) {
                return;
            }

            runCheck();
        }, 15 * 60_000);

        // Ao voltar para o TCloud, verifica se já passou tempo suficiente.
        const handleVisibilityChange = () => {
            if (document.visibilityState !== 'visible') {
                return;
            }

            if (Date.now() - lastCheckRef.current >= 60_000) {
                runCheck();
            }
        };

        // Também cobre casos de minimizar/restaurar a janela.
        const handleFocus = () => {
            if (Date.now() - lastCheckRef.current >= 60_000) {
                runCheck();
            }
        };

        document.addEventListener(
            'visibilitychange',
            handleVisibilityChange
        );

        window.addEventListener('focus', handleFocus);

        return () => {
            disposed = true;

            window.clearTimeout(startupTimer);
            window.clearInterval(interval);

            document.removeEventListener(
                'visibilitychange',
                handleVisibilityChange
            );

            window.removeEventListener('focus', handleFocus);
        };
    }, [checkForUpdates]);

    return {
        ...state,
        checkForUpdates,
        downloadAndInstall,
        dismissUpdate,
    };
}