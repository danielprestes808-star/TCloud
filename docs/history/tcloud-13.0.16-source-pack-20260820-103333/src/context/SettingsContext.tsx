import { createContext, useContext, useState, useEffect, ReactNode, useCallback, useRef } from 'react';
import { load } from '@tauri-apps/plugin-store';
import { LanguagePreference } from '../i18n/languages';

export type ExplorerViewMode = 'extra-large' | 'large' | 'medium' | 'small' | 'list' | 'details' | 'gallery';

export interface Settings {
    viewMode: ExplorerViewMode;
    autoUpdate: boolean;
    maxConcurrentUploads: number;
    maxConcurrentDownloads: number;
    zipFolders: boolean;
    language: LanguagePreference;

    // ── Proxy ──────────────────────────────────────────────
    proxyEnabled: boolean;
    proxyType: 'socks5' | 'http' | 'https';  // SOCKS5 or HTTP/HTTPS via local SOCKS5 bridge
    proxyHost: string;
    proxyPort: number;
    proxyUsername: string;
    proxyPassword: string;   // SOCKS5
    proxyLiveStateEnabled: boolean;

    // ── Sidebar ─────────────────────────────────────────────
    sidebarCollapsed: boolean;
    hideGroups: boolean;

    // ── VPN Optimizer (master toggle) ─────────────────────
    vpnMode: boolean;

    // Individual controls (active only when vpnMode = true)
    timeoutMultiplier: number;       // 1–5
    retryAttempts: number;           // 0–5
    retryBaseBackoffSec: number;     // 0.5–5
    retryMaxBackoffSec: number;      // 8–60
    adaptivePolling: boolean;
    pollingMinSec: number;           // 10–30
    pollingMaxSec: number;           // 45–120
    preferredDC: 'auto' | 'dc1' | 'dc2' | 'dc3' | 'dc4' | 'dc5';
    dcFallbackAttempts: number;      // 1–4
    floodWaitRespect: boolean;
    peerCacheSize: number;           // 100–2000
    bandwidthLimitUpKBs: number;     // 0 = unlimited, KB/s
    bandwidthLimitDownKBs: number;   // 0 = unlimited, KB/s
    chunkSizeKb: number;             // 128, 256, 512
    keepAliveIntervalSec: number;    // 0 = disabled, 30–120
    autoDetectVpn: boolean;
    archiveMaxBytes: number;           // 0 = unlimited, MiB for bulk archive (API)

    // ── Performance ────────────────────────────────────────
    performanceMode: boolean;        // Disable blur, shadows, and heavy animations
    linuxRenderingFix: boolean;      // WEBKIT_DISABLE_DMABUF_RENDERER=1 (Linux only, restart required)

    // ── Transcode cache ─────────────────────────────────────
    transcodeCacheMaxGb: number;     // 1–50 GB, default 5

    // ── Encryption ────────────────────────────────────────
    encryptionDefaultMode: 'standard' | 'vault' | 'passphrase' | 'vault_and_passphrase';
    encryptionProtectMetadata: boolean;
    encryptionAutoLockMinutes: number;
    encryptionLockOnSleep: boolean;
    encryptionTempPolicy: 'balanced' | 'strict';
}

const defaultSettings: Settings = {
    viewMode: 'medium',
    autoUpdate: true,
    maxConcurrentUploads: 8,
    maxConcurrentDownloads: 8,
    zipFolders: true,
    language: 'system',

    // Proxy — off by default
    proxyEnabled: false,
    proxyType: 'socks5',
    proxyHost: '',
    proxyPort: 1080,
    proxyUsername: '',
    proxyPassword: '',
    proxyLiveStateEnabled: true,

    // Sidebar
    sidebarCollapsed: false,
    hideGroups: false,

    // VPN Optimizer — off by default (preserves existing behaviour)
    vpnMode: false,
    timeoutMultiplier: 3,
    retryAttempts: 3,
    retryBaseBackoffSec: 1,
    retryMaxBackoffSec: 30,
    adaptivePolling: true,
    pollingMinSec: 15,
    pollingMaxSec: 60,
    preferredDC: 'auto',
    dcFallbackAttempts: 2,
    floodWaitRespect: true,
    peerCacheSize: 500,
    bandwidthLimitUpKBs: 0,
    bandwidthLimitDownKBs: 0,
    chunkSizeKb: 512,
    keepAliveIntervalSec: 0,
    autoDetectVpn: false,
    archiveMaxBytes: 256,  // 256 MiB

    performanceMode: false,
    linuxRenderingFix: true,

    transcodeCacheMaxGb: 5,

    // Encryption — off by default
    encryptionDefaultMode: 'standard',
    encryptionProtectMetadata: true,
    encryptionAutoLockMinutes: 15,
    encryptionLockOnSleep: true,
    encryptionTempPolicy: 'balanced',
};

interface SettingsContextType {
    settings: Settings;
    updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
    resetSettings: () => void;
    isLoaded: boolean;
}

const SettingsContext = createContext<SettingsContextType | undefined>(undefined);

export function SettingsProvider({ children }: { children: ReactNode }) {
    const [settings, setSettings] = useState<Settings>(defaultSettings);
    const [isLoaded, setIsLoaded] = useState(false);
    const storeRef = useRef<Awaited<ReturnType<typeof load>> | null>(null);
    const persistTimerRef = useRef<number | null>(null);
    const pendingSettingsRef = useRef<Settings | null>(null);

    // Load settings from Tauri store on mount
    useEffect(() => {
        const loadSettings = async () => {
            try {
                const store = await load('settings.json');
                storeRef.current = store;
                const saved = await store.get<Settings>('settings');
                if (saved) {
                    // Merge with defaults so new keys are always present
                    const merged = { ...defaultSettings, ...saved };
                    // Backward compat: map old 'mtproto' proxyType to 'socks5'
                    if ((merged.proxyType as string) === 'mtproto') {
                        merged.proxyType = 'socks5';
                    }
                    // Explorer 11.1: migrate the old two-state grid/list preference.
                    if ((merged.viewMode as string) === 'grid') {
                        merged.viewMode = 'medium';
                    }

                    // TCloud: o perfil de desempenho é a fonte única de concorrência.
                    const performanceProfile = localStorage.getItem('tcloud:performance-profile');
                    const profileConcurrency =
                        performanceProfile === 'economy'
                            ? 2
                            : 8;

                    merged.maxConcurrentUploads = profileConcurrency;
                    merged.maxConcurrentDownloads = profileConcurrency;

                    setSettings(merged);
                }
            } catch {
                // Store not available or first run — use defaults
            } finally {
                setIsLoaded(true);
            }
        };
        loadSettings();
    }, []);

    const persistSettings = useCallback((next: Settings) => {
        pendingSettingsRef.current = next;
        if (persistTimerRef.current !== null) window.clearTimeout(persistTimerRef.current);
        persistTimerRef.current = window.setTimeout(async () => {
            persistTimerRef.current = null;
            const value = pendingSettingsRef.current;
            if (!value) return;
            try {
                const store = storeRef.current ?? await load('settings.json');
                storeRef.current = store;
                await store.set('settings', value);
                await store.save();
            } catch {
                // best-effort persistence
            }
        }, 220);
    }, []);

    useEffect(() => () => {
        if (persistTimerRef.current !== null) window.clearTimeout(persistTimerRef.current);
    }, []);
    useEffect(() => {
        const handlePerformanceProfileChanged = (event: Event) => {
            const customEvent = event as CustomEvent<{ profile?: string }>;
            const profile = customEvent.detail?.profile;

            const concurrency =
                profile === 'economy'
                    ? 2
                    : profile === 'performance'
                        ? 8
                        : 4;

            setSettings(prev => {
                if (
                    prev.maxConcurrentUploads === concurrency &&
                    prev.maxConcurrentDownloads === concurrency
                ) {
                    return prev;
                }

                const next = {
                    ...prev,
                    maxConcurrentUploads: concurrency,
                    maxConcurrentDownloads: concurrency,
                };

                persistSettings(next);
                return next;
            });
        };

        window.addEventListener(
            'tcloud-performance-profile-changed',
            handlePerformanceProfileChanged
        );

        return () => {
            window.removeEventListener(
                'tcloud-performance-profile-changed',
                handlePerformanceProfileChanged
            );
        };
    }, [persistSettings]);

    const updateSetting = useCallback(<K extends keyof Settings>(key: K, value: Settings[K]) => {
        setSettings(prev => {
            const next = { ...prev, [key]: value };
            persistSettings(next);
            return next;
        });
    }, [persistSettings]);

    const resetSettings = useCallback(() => {
        setSettings(defaultSettings);
        persistSettings(defaultSettings);
    }, [persistSettings]);

    return (
        <SettingsContext.Provider value={{ settings, updateSetting, resetSettings, isLoaded }}>
            {children}
        </SettingsContext.Provider>
    );
}

export const useSettings = () => {
    const context = useContext(SettingsContext);
    if (!context) throw new Error('useSettings must be used within a SettingsProvider');
    return context;
};
