import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { getTCloudBackgroundWorkDelay, getTCloudUiResumeDelay, isTCloudUiBackgroundReady } from './tcloudWindowResume';

type CacheEntry = {
    src: string;
    cachedAt: number;
};

type QueueTask = {
    key: string;
    background: boolean;
    run: () => Promise<void>;
    cancel: () => void;
};

const CACHE_TTL_MS = 2 * 60 * 60 * 1000;
const PREVIEW_CACHE_MAX_ITEMS = 24;
const THUMBNAIL_CACHE_MAX_ITEMS = 2048;
let thumbnailConcurrency = Number(localStorage.getItem('tcloud:thumbnail-concurrency') || '4');
const PREVIEW_CONCURRENCY = 2;

const previewCache = new Map<string, CacheEntry>();
const thumbnailCache = new Map<string, CacheEntry>();
const pendingPreviews = new Map<string, Promise<string | null>>();
const pendingThumbnails = new Map<string, Promise<string | null>>();

// 12.7.1 Part 2: visible-card work always jumps ahead of speculative prefetch.
// Background thumbnail work also pauses while the desktop window is hidden so
// tray/background sync does not waste CPU/network rendering images nobody sees.
const thumbnailForegroundQueue: QueueTask[] = [];
const thumbnailBackgroundQueue: QueueTask[] = [];
const previewForegroundQueue: QueueTask[] = [];
const previewBackgroundQueue: QueueTask[] = [];
let activeThumbnailLoads = 0;
let activeBackgroundThumbnailLoads = 0;
let activePreviewLoads = 0;
let activeBackgroundPreviewLoads = 0;
let thumbnailPumpTimer: number | null = null;
let thumbnailPumpAt = 0;
let previewPumpTimer: number | null = null;
let previewPumpAt = 0;
let hiddenTrimTimer: number | null = null;

const scheduleThumbnailPump = (delayMs: number) => {
    if (typeof window === 'undefined') return;
    const safeDelay = Math.max(16, delayMs);
    const target = performance.now() + safeDelay;
    if (thumbnailPumpTimer !== null && target >= thumbnailPumpAt - 4) return;
    if (thumbnailPumpTimer !== null) window.clearTimeout(thumbnailPumpTimer);
    thumbnailPumpAt = target;
    thumbnailPumpTimer = window.setTimeout(() => {
        thumbnailPumpTimer = null;
        thumbnailPumpAt = 0;
        pumpThumbnailQueue();
    }, safeDelay);
};


const schedulePreviewPump = (delayMs: number) => {
    if (typeof window === 'undefined') return;
    const safeDelay = Math.max(24, delayMs);
    const target = performance.now() + safeDelay;
    if (previewPumpTimer !== null && target >= previewPumpAt - 4) return;
    if (previewPumpTimer !== null) window.clearTimeout(previewPumpTimer);
    previewPumpAt = target;
    previewPumpTimer = window.setTimeout(() => {
        previewPumpTimer = null;
        previewPumpAt = 0;
        pumpPreviewQueue();
    }, safeDelay);
};

window.addEventListener('tcloud-performance-profile-changed', (event) => {
    const detail = (event as CustomEvent<{ thumbnailConcurrency?: number }>).detail;
    const value = Number(detail?.thumbnailConcurrency || localStorage.getItem('tcloud:thumbnail-concurrency') || '4');
    thumbnailConcurrency = Math.max(1, Math.min(10, value));
    pumpThumbnailQueue();
});

document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
        if (hiddenTrimTimer !== null) {
            window.clearTimeout(hiddenTrimTimer);
            hiddenTrimTimer = null;
        }
        scheduleThumbnailPump(getTCloudUiResumeDelay() + 24);
        schedulePreviewPump(getTCloudUiResumeDelay() + 24);
        return;
    }

    // A minimized/hidden TCloud should not keep speculative image work or a
    // large decoded-path LRU alive forever. The persistent disk caches remain,
    // so returning to the app is still fast without holding unnecessary RAM.
    if (hiddenTrimTimer !== null) window.clearTimeout(hiddenTrimTimer);
    hiddenTrimTimer = window.setTimeout(() => {
        hiddenTrimTimer = null;
        reduceImageMemoryCachesForBackground();
    }, 60_000);
});

window.addEventListener('tcloud-ui-resume', () => {
    scheduleThumbnailPump(getTCloudUiResumeDelay() + 24);
    schedulePreviewPump(getTCloudUiResumeDelay() + 24);
});

export const getImageCacheKey = (fileId: number, folderId?: number | null) =>
    `${folderId ?? 'home'}:${fileId}`;

const normalizeAssetSource = (value: string): string => {
    if (/^(?:data:|blob:|asset:|https?:)/i.test(value)) return value;
    return convertFileSrc(value);
};

const remember = (cache: Map<string, CacheEntry>, key: string, src: string, maxItems: number) => {
    if (cache.has(key)) cache.delete(key);
    cache.set(key, { src, cachedAt: Date.now() });
    while (cache.size > maxItems) {
        const oldestKey = cache.keys().next().value;
        if (!oldestKey) break;
        cache.delete(oldestKey);
    }
};

const read = (cache: Map<string, CacheEntry>, key: string, maxItems: number): string | null => {
    const entry = cache.get(key);
    if (!entry) return null;
    if (Date.now() - entry.cachedAt > CACHE_TTL_MS) {
        cache.delete(key);
        return null;
    }
    // LRU touch.
    remember(cache, key, entry.src, maxItems);
    return entry.src;
};

function pumpThumbnailQueue() {
    if (document.visibilityState !== 'visible') return;

    const foregroundDelay = getTCloudUiResumeDelay();
    if (foregroundDelay > 0) {
        scheduleThumbnailPump(foregroundDelay + 24);
        return;
    }

    while (activeThumbnailLoads < thumbnailConcurrency) {
        let task = thumbnailForegroundQueue.shift();
        let isBackground = false;

        if (!task) {
            if (thumbnailBackgroundQueue.length === 0) break;

            // Speculative work should never race the first restored frame or occupy
            // every thumbnail slot. One background request at a time is enough to
            // keep likely-next items warm without making scrolling/restoring sluggish.
            if (!isTCloudUiBackgroundReady(900)) {
                scheduleThumbnailPump(getTCloudBackgroundWorkDelay(900) + 24);
                break;
            }
            if (activeBackgroundThumbnailLoads >= 1) break;
            task = thumbnailBackgroundQueue.shift();
            isBackground = true;
        }

        if (!task) break;
        activeThumbnailLoads += 1;
        if (isBackground) activeBackgroundThumbnailLoads += 1;

        void task.run().finally(() => {
            activeThumbnailLoads -= 1;
            if (isBackground) activeBackgroundThumbnailLoads -= 1;
            pumpThumbnailQueue();
        });
    }
}

function pumpPreviewQueue() {
    if (document.visibilityState !== 'visible') return;

    while (activePreviewLoads < PREVIEW_CONCURRENCY) {
        let task = previewForegroundQueue.shift();
        let isBackground = false;

        if (!task) {
            if (previewBackgroundQueue.length === 0) break;
            // A speculative next-image preview must never occupy both preview
            // slots or race the first frame after restore/focus.
            if (!isTCloudUiBackgroundReady(1200)) {
                schedulePreviewPump(getTCloudBackgroundWorkDelay(1200) + 24);
                break;
            }
            if (activeBackgroundPreviewLoads >= 1) break;
            task = previewBackgroundQueue.shift();
            isBackground = true;
        }

        if (!task) break;
        activePreviewLoads += 1;
        if (isBackground) activeBackgroundPreviewLoads += 1;
        void task.run().finally(() => {
            activePreviewLoads -= 1;
            if (isBackground) activeBackgroundPreviewLoads -= 1;
            pumpPreviewQueue();
        });
    }
}

const promoteQueuedPreview = (key: string) => {
    const index = previewBackgroundQueue.findIndex((task) => task.key === key);
    if (index < 0) return;
    const [task] = previewBackgroundQueue.splice(index, 1);
    if (!task) return;
    task.background = false;
    previewForegroundQueue.unshift(task);
};

const enqueueInvoke = (
    command: 'cmd_get_preview' | 'cmd_get_thumbnail',
    fileId: number,
    folderId: number | null | undefined,
    cache: Map<string, CacheEntry>,
    pending: Map<string, Promise<string | null>>,
    maxItems: number,
    background = false,
): Promise<string | null> => {
    const key = getImageCacheKey(fileId, folderId);
    const cached = read(cache, key, maxItems);
    if (cached) return Promise.resolve(cached);

    const existing = pending.get(key);
    if (existing) {
        if (command === 'cmd_get_preview' && !background) {
            promoteQueuedPreview(key);
            pumpPreviewQueue();
        }
        return existing;
    }

    let resolveRequest!: (value: string | null) => void;
    let rejectRequest!: (reason?: unknown) => void;
    const request = new Promise<string | null>((resolve, reject) => {
        resolveRequest = resolve;
        rejectRequest = reject;
    });
    pending.set(key, request);

    const task: QueueTask = {
        key,
        background,
        cancel: () => {
            pending.delete(key);
            resolveRequest(null);
        },
        run: async () => {
            try {
                // Re-check after waiting in queue because another path may have populated cache.
                const afterWait = read(cache, key, maxItems);
                if (afterWait) {
                    resolveRequest(afterWait);
                    return;
                }

                const args = command === 'cmd_get_thumbnail'
                    ? { messageId: fileId, folderId: folderId ?? null, background }
                    : { messageId: fileId, folderId: folderId ?? null };
                const path = await invoke<string>(command, args);
                if (!path) {
                    resolveRequest(null);
                    return;
                }

                const src = normalizeAssetSource(path);
                remember(cache, key, src, maxItems);
                resolveRequest(src);
            } catch (error) {
                rejectRequest(error);
            } finally {
                pending.delete(key);
            }
        },
    };

    if (command === 'cmd_get_thumbnail') {
        (background ? thumbnailBackgroundQueue : thumbnailForegroundQueue).push(task);
        pumpThumbnailQueue();
    } else {
        (background ? previewBackgroundQueue : previewForegroundQueue).push(task);
        pumpPreviewQueue();
    }
    return request;
};

export const getCachedPreview = (fileId: number, folderId?: number | null) =>
    read(previewCache, getImageCacheKey(fileId, folderId), PREVIEW_CACHE_MAX_ITEMS);

export const getCachedThumbnail = (fileId: number, folderId?: number | null) =>
    read(thumbnailCache, getImageCacheKey(fileId, folderId), THUMBNAIL_CACHE_MAX_ITEMS);

export const loadPreview = (fileId: number, folderId?: number | null) =>
    enqueueInvoke('cmd_get_preview', fileId, folderId, previewCache, pendingPreviews, PREVIEW_CACHE_MAX_ITEMS, false);

export const prefetchPreview = (fileId: number, folderId?: number | null) => {
    if (getCachedPreview(fileId, folderId)) return;
    void enqueueInvoke(
        'cmd_get_preview',
        fileId,
        folderId,
        previewCache,
        pendingPreviews,
        PREVIEW_CACHE_MAX_ITEMS,
        true,
    ).catch(() => {});
};

export const loadThumbnail = (fileId: number, folderId?: number | null) =>
    enqueueInvoke('cmd_get_thumbnail', fileId, folderId, thumbnailCache, pendingThumbnails, THUMBNAIL_CACHE_MAX_ITEMS, false);

export const prefetchThumbnail = (fileId: number, folderId?: number | null) => {
    if (getCachedThumbnail(fileId, folderId)) return;
    // Do not compete with the current render/scroll. The native side will also
    // defer this request when Explorer has an active high-priority transfer.
    window.setTimeout(() => {
        void enqueueInvoke(
            'cmd_get_thumbnail',
            fileId,
            folderId,
            thumbnailCache,
            pendingThumbnails,
            THUMBNAIL_CACHE_MAX_ITEMS,
            true,
        ).catch(() => {});
    }, 120);
};

export const forgetPreview = (fileId: number, folderId?: number | null) => {
    previewCache.delete(getImageCacheKey(fileId, folderId));
};

export const forgetThumbnail = (fileId: number, folderId?: number | null) => {
    thumbnailCache.delete(getImageCacheKey(fileId, folderId));
};

const trimCache = (cache: Map<string, CacheEntry>, keep: number) => {
    while (cache.size > keep) {
        const oldestKey = cache.keys().next().value;
        if (!oldestKey) break;
        cache.delete(oldestKey);
    }
};

const cancelQueuedBackgroundTasks = (queue: QueueTask[]) => {
    while (queue.length > 0) {
        queue.shift()?.cancel();
    }
};

export const reduceImageMemoryCachesForBackground = () => {
    // Keep a small warm set so restore remains visually immediate while dropping
    // the bulk of path metadata accumulated during long browsing sessions.
    trimCache(previewCache, 4);
    trimCache(thumbnailCache, 64);
    cancelQueuedBackgroundTasks(previewBackgroundQueue);
    cancelQueuedBackgroundTasks(thumbnailBackgroundQueue);
};

export const clearImageMemoryCaches = () => {
    previewCache.clear();
    thumbnailCache.clear();
    cancelQueuedBackgroundTasks(previewBackgroundQueue);
    cancelQueuedBackgroundTasks(thumbnailBackgroundQueue);
};
