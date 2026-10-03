import { AssetStatus } from './generated-types/AssetStatus';

export interface StatusPresentation {
  label: string;
  badge: string;
}

/**
 * Opis statusów z rozdziału 5. `Record<AssetStatus, …>` wymusza obsługę
 * każdego statusu dodanego w Ruście (typ generowany przez ts-rs).
 */
export const STATUS_PRESENTATION: Record<AssetStatus, StatusPresentation> = {
  UPLOADING: { label: 'Wysyłanie', badge: 'badge-ghost' },
  QUARANTINED: { label: 'W kwarantannie', badge: 'badge-info' },
  SCANNING: { label: 'Skanowanie', badge: 'badge-info' },
  REJECTED: { label: 'Odrzucony', badge: 'badge-error' },
  INFECTED: { label: 'Zainfekowany', badge: 'badge-error' },
  SCAN_FAILED: { label: 'Błąd skanu', badge: 'badge-warning' },
  CLEAN_DRAFT: { label: 'Czeka na publikację', badge: 'badge-success' },
  PUBLISHED: { label: 'Opublikowany', badge: 'badge-primary' },
  ARCHIVED: { label: 'Zarchiwizowany', badge: 'badge-neutral' },
};

/** Statusy, które pipeline jeszcze zmieni (warto odświeżać listę). */
export function isInProgress(status: AssetStatus): boolean {
  return status === 'QUARANTINED' || status === 'SCANNING';
}
