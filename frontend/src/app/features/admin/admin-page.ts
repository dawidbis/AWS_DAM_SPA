import { Component, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AssetPager } from '../../core/api/asset-pager';
import { AssetsService } from '../../core/api/assets.service';
import { DeleteAssetService } from '../../core/api/delete-asset.service';
import { DictionariesService } from '../../core/api/dictionaries.service';
import { DownloadService } from '../../core/api/download.service';
import { AssetSummary } from '../../core/api/generated-types/AssetSummary';
import { AssetCard } from '../assets/asset-card';
import { MetadataEditor } from '../assets/metadata-editor';

/**
 * Panel A: kolejka publikacji (czyste pliki po skanie, opis metadanych przed
 * publikacją) i ponawianie nieudanych skanów.
 */
@Component({
  selector: 'app-admin-page',
  imports: [AssetCard, MetadataEditor, RouterLink],
  template: `
    <div class="mb-4 flex items-center justify-between gap-2">
      <h2 class="text-2xl font-bold">Do publikacji</h2>
      <div class="flex gap-2">
        <a class="btn btn-sm btn-ghost" routerLink="/admin/dictionaries">Słowniki</a>
        <button class="btn btn-sm" type="button" (click)="pager.reload()">Odśwież</button>
      </div>
    </div>
    <p class="mb-4 text-sm opacity-80">
      Pliki, które przeszły skan antywirusowy. Po publikacji są widoczne w galerii dla grup A i B.
    </p>

    @if (message(); as message) {
      <div class="alert mb-4" [class.alert-success]="message.ok" [class.alert-error]="!message.ok">
        {{ message.text }}
      </div>
    }
    @if (deletion.message(); as message) {
      <div class="alert mb-4" [class.alert-success]="message.ok" [class.alert-error]="!message.ok">
        {{ message.text }}
      </div>
    }
    @if (downloads.error(); as error) {
      <div class="alert alert-error mb-4">{{ error }}</div>
    }
    @if (pager.state() === 'error') {
      <div class="alert alert-error mb-4">Nie udało się wczytać kolejki publikacji.</div>
    }

    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      @for (asset of pager.items(); track asset.assetId) {
        <app-asset-card [asset]="asset">
          <button
            class="btn btn-sm"
            type="button"
            [disabled]="downloads.pending() === asset.assetId"
            (click)="downloads.start(asset.assetId)"
          >
            Pobierz
          </button>
          <button
            class="btn btn-sm btn-error btn-outline"
            type="button"
            [disabled]="deletion.pending() === asset.assetId"
            (click)="deletion.remove(asset, () => pager.remove(asset.assetId))"
          >
            Usuń
          </button>
          <button class="btn btn-sm" type="button" (click)="editing.set(asset)">Opisz</button>
          <button
            class="btn btn-sm btn-primary"
            type="button"
            [disabled]="publishing() === asset.assetId"
            (click)="publish(asset.assetId)"
          >
            Publikuj
          </button>
        </app-asset-card>
      } @empty {
        @if (pager.state() === 'ready') {
          <p class="opacity-70">Brak plików czekających na publikację.</p>
        }
      }
    </div>

    @if (pager.state() === 'loading') {
      <span class="loading loading-spinner mt-4"></span>
    } @else if (pager.nextCursor()) {
      <button class="btn btn-sm mt-4" type="button" (click)="pager.more()">Załaduj więcej</button>
    }

    <div class="mt-10 mb-4 flex items-center justify-between gap-2">
      <h2 class="text-2xl font-bold">Błędy skanu</h2>
      <button class="btn btn-sm" type="button" (click)="failed.reload()">Odśwież</button>
    </div>
    <p class="mb-4 text-sm opacity-80">
      Skan się nie powiódł (błąd lub timeout), więc plik nie trafił do galerii. Plik zostaje w
      kwarantannie przez kilka dni i w tym czasie można ponowić skan.
    </p>
    @if (failed.state() === 'error') {
      <div class="alert alert-error mb-4">Nie udało się wczytać listy.</div>
    }
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      @for (asset of failed.items(); track asset.assetId) {
        <app-asset-card [asset]="asset" [showStatus]="true">
          <button
            class="btn btn-sm btn-error btn-outline"
            type="button"
            [disabled]="deletion.pending() === asset.assetId"
            (click)="deletion.remove(asset, () => failed.remove(asset.assetId))"
          >
            Usuń
          </button>
          <button
            class="btn btn-sm btn-warning"
            type="button"
            [disabled]="rescanning() === asset.assetId"
            (click)="rescan(asset.assetId)"
          >
            Skanuj ponownie
          </button>
        </app-asset-card>
      } @empty {
        @if (failed.state() === 'ready') {
          <p class="opacity-70">Brak plików z nieudanym skanem.</p>
        }
      }
    </div>
    @if (failed.nextCursor() && failed.state() !== 'loading') {
      <button class="btn btn-sm mt-4" type="button" (click)="failed.more()">Załaduj więcej</button>
    }

    @if (editing(); as asset) {
      <app-metadata-editor
        [asset]="asset"
        (saved)="onMetadataSaved($event)"
        (closed)="editing.set(null)"
      />
    }
  `,
})
export class AdminPage {
  protected readonly downloads = inject(DownloadService);
  protected readonly deletion = inject(DeleteAssetService);
  private readonly assets = inject(AssetsService);

  protected readonly pager = new AssetPager((cursor) => this.assets.list('drafts', cursor));
  protected readonly failed = new AssetPager((cursor) => this.assets.list('failed', cursor));
  protected readonly publishing = signal<string | null>(null);
  protected readonly rescanning = signal<string | null>(null);
  protected readonly message = signal<{ ok: boolean; text: string } | null>(null);
  /** Asset, którego metadane są edytowane. */
  protected readonly editing = signal<AssetSummary | null>(null);

  constructor() {
    inject(DictionariesService).load();
    this.pager.reload();
    this.failed.reload();
  }

  protected onMetadataSaved(asset: AssetSummary): void {
    this.pager.replace(asset);
    this.editing.set(null);
    this.message.set({ ok: true, text: 'Zapisano opis.' });
  }

  protected rescan(assetId: string): void {
    this.rescanning.set(assetId);
    this.message.set(null);
    this.assets.rescan(assetId).subscribe({
      next: () => {
        this.rescanning.set(null);
        this.failed.remove(assetId);
        this.message.set({ ok: true, text: 'Skan uruchomiony ponownie.' });
      },
      error: () => {
        this.rescanning.set(null);
        this.message.set({ ok: false, text: 'Nie udało się ponowić skanu. Odśwież listę.' });
      },
    });
  }

  protected publish(assetId: string): void {
    this.publishing.set(assetId);
    this.message.set(null);
    this.assets.publish(assetId).subscribe({
      next: () => {
        this.publishing.set(null);
        this.pager.remove(assetId);
        this.message.set({ ok: true, text: 'Opublikowano. Plik jest widoczny w galerii.' });
      },
      error: () => {
        this.publishing.set(null);
        this.message.set({
          ok: false,
          text: 'Nie udało się opublikować pliku. Odśwież listę i spróbuj ponownie.',
        });
      },
    });
  }
}
