import { X, Download, Loader2 } from 'lucide-react';

interface UpdateBannerProps {
    available: boolean;
    version: string | null;
    downloading: boolean;
    progress: number;
    onUpdate: () => void;
    onDismiss: () => void;
}

export function UpdateBanner({
    available,
    version,
    downloading,
    progress,
    onUpdate,
    onDismiss,
}: UpdateBannerProps) {
    if (!available) return null;

    return (
        <div className="absolute top-0 left-0 right-0 z-[100] px-4 pt-3 pointer-events-none">
            <div className="mx-auto max-w-3xl rounded-xl border border-telegram-border bg-telegram-panel/95 shadow-lg backdrop-blur-md pointer-events-auto">
                <div className="flex items-center gap-3 px-4 py-3">
                    <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-telegram-primary/10 text-telegram-primary">
                        {downloading ? (
                            <Loader2 className="h-5 w-5 animate-spin" />
                        ) : (
                            <Download className="h-5 w-5" />
                        )}
                    </div>

                    <div className="min-w-0 flex-1">
                        <p className="text-sm font-semibold text-telegram-text">
                            {downloading
                                ? `Atualizando o TCloud para ${version ?? 'a nova versão'}...`
                                : `Uma nova versão do TCloud (${version}) está disponível.`}
                        </p>

                        <p className="mt-0.5 text-xs text-telegram-subtext">
                            {downloading
                                ? progress > 0
                                    ? `Baixando e preparando a atualização: ${progress}%`
                                    : 'Preparando o download da atualização...'
                                : 'A atualização será baixada, verificada e instalada automaticamente.'}
                        </p>

                        {downloading && (
                            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-telegram-border">
                                <div
                                    className="h-full rounded-full bg-telegram-primary transition-all duration-300"
                                    style={{ width: `${Math.max(2, Math.min(progress, 100))}%` }}
                                />
                            </div>
                        )}
                    </div>

                    {!downloading && (
                        <button
                            type="button"
                            onClick={onUpdate}
                            className="shrink-0 rounded-lg bg-telegram-primary px-4 py-2 text-sm font-semibold text-white transition-opacity hover:opacity-90"
                        >
                            Atualizar agora
                        </button>
                    )}

                    {!downloading && (
                        <button
                            type="button"
                            onClick={onDismiss}
                            aria-label="Fechar aviso de atualização"
                            className="shrink-0 rounded-lg p-2 text-telegram-subtext transition-colors hover:bg-telegram-border/60 hover:text-telegram-text"
                        >
                            <X className="h-4 w-4" />
                        </button>
                    )}
                </div>
            </div>
        </div>
    );
}