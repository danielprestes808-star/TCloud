import { setTCloudTransferLimits } from './tcloudTransferLimits';

export type TCloudPerformanceProfile = 'economy' | 'balanced' | 'performance';
const KEY = 'tcloud:performance-profile';

export function getPerformanceProfile(): TCloudPerformanceProfile {
  const value = localStorage.getItem(KEY);
  return value === 'economy' || value === 'performance' ? value : 'balanced';
}

export async function applyPerformanceProfile(profile: TCloudPerformanceProfile) {
  localStorage.setItem(KEY, profile);
  const thumbnailConcurrency = profile === 'economy' ? 2 : profile === 'performance' ? 8 : 6;
  localStorage.setItem('tcloud:thumbnail-concurrency', String(thumbnailConcurrency));
  document.body.classList.add('tcloud-fluidity-mode');
  document.documentElement.dataset.tcloudPerformanceProfile = profile;
  window.dispatchEvent(new CustomEvent('tcloud-performance-profile-changed', { detail: { profile, thumbnailConcurrency } }));
  await setTCloudTransferLimits(profile === 'economy' ? 2 : 4).catch(() => null);
  return profile;
}

export function setAutoEconomyEnabled(enabled:boolean) {
  localStorage.setItem('tcloud:auto-economy', enabled ? 'true' : 'false');
}
export function getAutoEconomyEnabled() { return localStorage.getItem('tcloud:auto-economy') === 'true'; }


