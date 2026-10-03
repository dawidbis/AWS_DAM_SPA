import { DOCUMENT } from '@angular/common';
import { Injectable, InjectionToken, inject, signal } from '@angular/core';

import { AssetsService } from './assets.service';
import { AssetSummary } from './generated-types/AssetSummary';

/** Okno potwierdzenia przeglądarki (podmieniane w testach). */
export const BROWSER_CONFIRM = new InjectionToken<(message: string) => boolean>('BROWSER_CONFIRM', {
  providedIn: 'root',
  factory: () => {
    const view = inject(DOCUMENT).defaultView;
    return (message: string) => view?.confirm(message) ?? false;
  },
});

/** Usuwanie assetu po potwierdzeniu (A). Uprawnienia sprawdza API. */
@Injectable({ providedIn: 'root' })
export class DeleteAssetService {
  private readonly assets = inject(AssetsService);
  private readonly confirm = inject(BROWSER_CONFIRM);

  /** Identyfikator assetu, którego usuwanie trwa. */
  readonly pending = signal<string | null>(null);
  readonly message = signal<{ ok: boolean; text: string } | null>(null);

  remove(asset: AssetSummary, onDeleted: () => void): void {
    const name = asset.title || asset.originalFilename;
    if (!this.confirm(`Usunąć „${name}”? Asset zniknie z galerii i list.`)) {
      return;
    }
    this.pending.set(asset.assetId);
    this.message.set(null);
    this.assets.remove(asset.assetId).subscribe({
      next: () => {
        this.pending.set(null);
        this.message.set({ ok: true, text: `Usunięto „${name}”.` });
        onDeleted();
      },
      error: () => {
        this.pending.set(null);
        this.message.set({ ok: false, text: 'Nie udało się usunąć assetu. Odśwież listę.' });
      },
    });
  }
}
