import React, { useState, useEffect, Suspense } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ErrorBoundary } from "./components/shared/ErrorBoundary";
import { BrandSplash } from "./components/shared/BrandSplash";
import { UpdateBanner } from "./components/shared/UpdateBanner";
import { useUpdateCheck } from "./hooks/useUpdateCheck";
import "./App.css";
import "./styles/tcloud-explorer.css";

const DesktopDashboard = React.lazy(() => import("./components/desktop/DesktopDashboard").then(m => ({ default: m.Dashboard })));
const AuthWizard = React.lazy(() => import("./components/shared/AuthWizard").then(m => ({ default: m.AuthWizard })));
const DesignGallery = import.meta.env.DEV
  ? React.lazy(() => import("./components/dev/DesignGallery"))
  : null;

import { Toaster } from "sonner";
import { ConfirmProvider } from "./context/ConfirmContext";
import { ThemeProvider, useTheme } from "./context/ThemeContext";
import { SettingsProvider } from "./context/SettingsContext";
import { EncryptionProvider } from "./hooks/useEncryption.tsx";
import { useSettings } from "./context/SettingsContext";
import { useTranslation } from "react-i18next";

import { getLanguageInfo } from "./i18n/languages";
import { resolveLanguagePreference } from "./i18n/resolveLanguage";
import { installClientDiagnostics } from "./services/tcloudDiagnostics";
import { applyPerformanceProfile, getAutoEconomyEnabled, getPerformanceProfile } from "./services/tcloudPerformance";
import { markUpdateHealthy, runUpdateStartupGuard } from "./services/tcloudCompletion";
import { exit } from "@tauri-apps/plugin-process";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 30_000,
      gcTime: 30 * 60_000,
    },
    mutations: { retry: 0 },
  },
});

type AuthStatus = "loading" | "authenticated" | "unauthenticated";

function AppContent() {
  const [authStatus, setAuthStatus] = useState<AuthStatus>("loading");
  const { theme } = useTheme();
  const { available, version, downloading, progress, downloadAndInstall, dismissUpdate } = useUpdateCheck();
  const { settings, updateSetting, isLoaded } = useSettings();
  const { i18n } = useTranslation();

  useEffect(() => {
    installClientDiagnostics();
    void applyPerformanceProfile(getPerformanceProfile());
    let cancelled = false;
    let healthyTimer: number | null = null;

    void runUpdateStartupGuard().then((guard) => {
      if (cancelled) return;
      if (guard.action === 'rollback_scheduled') {
        void exit(0);
        return;
      }
      healthyTimer = window.setTimeout(() => { void markUpdateHealthy().catch(() => {}); }, 20000);
    }).catch(() => {
      healthyTimer = window.setTimeout(() => { void markUpdateHealthy().catch(() => {}); }, 20000);
    });

    if (!getAutoEconomyEnabled()) {
      return () => {
        cancelled = true;
        if (healthyTimer !== null) window.clearTimeout(healthyTimer);
      };
    }

    let cleanup: (() => void) | undefined;
    const setupBattery = async () => {
      const nav = navigator as Navigator & { getBattery?: () => Promise<{ charging:boolean; level:number; addEventListener:(name:string, fn:()=>void)=>void; removeEventListener:(name:string, fn:()=>void)=>void; }> };
      if (!nav.getBattery) return;
      const battery = await nav.getBattery();
      const update = () => {
        if (!battery.charging && battery.level <= 0.35) void applyPerformanceProfile('economy');
      };
      update();
      battery.addEventListener('levelchange', update);
      battery.addEventListener('chargingchange', update);
      cleanup = () => { battery.removeEventListener('levelchange', update); battery.removeEventListener('chargingchange', update); };
    };
    void setupBattery();
    return () => {
      cancelled = true;
      if (healthyTimer !== null) window.clearTimeout(healthyTimer);
      cleanup?.();
    };
  }, []);

  // Handle active language and RTL direction changes
  useEffect(() => {
    if (!isLoaded) return;
    const activeLang = resolveLanguagePreference(settings.language);
    const info = getLanguageInfo(activeLang);
    i18n.changeLanguage(activeLang);
    document.documentElement.lang = activeLang;
    document.documentElement.dir = info.dir;
  }, [settings.language, isLoaded, i18n]);

  // Performance mode: auto-enable when user has prefers-reduced-motion
  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    if (mediaQuery.matches && !settings.performanceMode) {
      updateSetting('performanceMode', true);
    }
    const handler = (e: MediaQueryListEvent) => {
      if (e.matches && !settings.performanceMode) {
        updateSetting('performanceMode', true);
      }
    };
    mediaQuery.addEventListener('change', handler);
    return () => mediaQuery.removeEventListener('change', handler);
  }, []);

  // Apply performance-mode class to body (guarded by settings load to avoid flicker)
  useEffect(() => {
    if (!isLoaded) return;
    if (settings.performanceMode) {
      document.body.classList.add('performance-mode');
    } else {
      document.body.classList.remove('performance-mode');
    }
  }, [settings.performanceMode, isLoaded]);

  // Fast startup:
  // decide the initial screen from the persisted local Telegram session.
  // Network initialization and validation continue in background.
  useEffect(() => {
    let cancelled = false;

    const restoreSession = async () => {
      try {
        const hasSavedSession =
          await invoke<boolean>("cmd_has_saved_session");

        if (cancelled) return;

        if (!hasSavedSession) {
          setAuthStatus("unauthenticated");
          return;
        }

        // Show dashboard immediately; Telegram reconnects in background.
        setAuthStatus("authenticated");

        void (async () => {
          try {
            await invoke("cmd_connect_simple");

            const ok =
              await invoke<boolean>("cmd_check_connection");

            if (!ok) {
              console.warn(
                "[TCloud] Telegram session temporarily unavailable."
              );
            }
          } catch (err) {
            console.warn(
              "[TCloud] Background Telegram restore failed:",
              err
            );
          }
        })();

      } catch (err) {

        console.warn(
          "[TCloud] Could not inspect saved session:",
          err
        );

        if (!cancelled) {
          setAuthStatus("unauthenticated");
        }
      }
    };

    void restoreSession();

    return () => {
      cancelled = true;
    };
  }, []);

  // Donation/support UI and legacy post-click thank-you flow removed from TCloud.

  // Optional housekeeping: run at most once every 24h after authentication.
  useEffect(() => {
    if (authStatus !== "authenticated" || localStorage.getItem('tcloud:auto-cleanup') !== 'true') return;
    const last = Number(localStorage.getItem('tcloud:auto-cleanup-last') || '0');
    if (Date.now() - last < 24 * 60 * 60 * 1000) return;
    const timer = window.setTimeout(() => {
      void invoke('tcloud_completion_cleanup_apply', { cacheDays: 30, trashDays: 30, versionDays: 90 })
        .then(() => localStorage.setItem('tcloud:auto-cleanup-last', String(Date.now())))
        .catch((error) => console.warn('[TCloud] automatic cleanup skipped:', error));
    }, 90_000);
    return () => window.clearTimeout(timer);
  }, [authStatus]);

  // Branded splash while restoring the persisted Telegram session.
  if (authStatus === "loading") {
    return <BrandSplash message="Restaurando sua sessão com segurança…" />;
  }

  return (
    <main className="absolute inset-0 text-telegram-text overflow-hidden selection:bg-telegram-primary/30">
      <UpdateBanner
        available={available}
        version={version}
        downloading={downloading}
        progress={progress}
        onUpdate={downloadAndInstall}
        onDismiss={dismissUpdate}
      />
      <Toaster
        theme={theme}
        position="bottom-right"
        visibleToasts={2}
        expand={false}
        toastOptions={{ duration: 2000 }}
      />

      {authStatus === "authenticated" && (
        <Suspense
          fallback={
            <div className="h-screen w-screen flex flex-col items-center justify-center bg-telegram-bg">
              <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-telegram-primary" />
            </div>
          }
        >
          <ErrorBoundary>
            <DesktopDashboard onLogout={() => setAuthStatus("unauthenticated")} />
          </ErrorBoundary>
        </Suspense>
      )}

      {authStatus === "unauthenticated" && (
        <Suspense fallback={<BrandSplash message="Carregando acesso…" />}>
          <AuthWizard onLogin={() => setAuthStatus("authenticated")} />
        </Suspense>
      )}
    </main>
  );
}


function App() {
  const showDesignGallery = Boolean(
    DesignGallery && typeof window !== 'undefined' && new URLSearchParams(window.location.search).has('design-gallery')
  );

  return (
    <ErrorBoundary>
      <ThemeProvider>
        <QueryClientProvider client={queryClient}>
          <ConfirmProvider>
            <SettingsProvider>
              <EncryptionProvider>
              {showDesignGallery && DesignGallery ? (
                <Suspense fallback={<div className="h-screen bg-app-canvas" />}>
                  <DesignGallery />
                </Suspense>
              ) : (
                <AppContent />
              )}
              </EncryptionProvider>
            </SettingsProvider>
          </ConfirmProvider>
        </QueryClientProvider>
      </ThemeProvider>
    </ErrorBoundary>
  );
}

export default App;
