import { Component, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AssetPager } from '../../core/api/asset-pager';
import { AssetsService } from '../../core/api/assets.service';
import { DeleteAssetService } from '../../core/api/delete-asset.service';
import { DictionariesService } from '../../core/api/dictionaries.service';
import { DownloadService } from '../../core/api/download.service';
import { AssetSummary } from '../../core/api/generated-types/AssetSummary';
import { AuthService } from '../../core/auth/auth.service';
import { AssetCard } from '../assets/asset-card';
import { MetadataEditor } from '../assets/metadata-editor';

/**
 * Galeria opublikowanych materiałów: A i B widzą miniatury i pobierają
 * oryginały, D widzi wyłącznie podglądy ze znakiem wodnym (bez pobierania).
 */
@Component({
  selector: 'app-gallery-page',
  imports: [AssetCard, MetadataEditor, RouterLink],
  template: `
    <div class="mb-4 flex items-center justify-between gap-2">
      <h2 class="text-2xl font-bold">Galeria</h2>
      @if (canBrowse) {
        <button class="btn btn-sm" type="button" (click)="pager.reload()">Odśwież</button>
      }
    </div>

    @if (!canBrowse) {
      <div class="alert" data-testid="gallery-unavailable">
        <span>
          Jako fotograf widzisz tylko własne materiały:
          <a class="link" routerLink="/my-submissions">Moje zgłoszenia</a>.
        </span>
      </div>
    } @else {
      @if (!canDownload) {
        <div class="alert alert-info mb-4" data-testid="watermark-info">
          Podglądy w niskiej rozdzielczości ze znakiem wodnym. Pełne pliki udostępnia dział
          komunikacji klubu.
        </div>
      }
      @if (downloads.error(); as error) {
        <div class="alert alert-error mb-4">{{ error }}</div>
      }
      @if (deletion.message(); as message) {
        <div
          class="alert mb-4"
          [class.alert-success]="message.ok"
          [class.alert-error]="!message.ok"
        >
          {{ message.text }}
        </div>
      }
      @switch (pager.state()) {
        @case ('error') {
          <div class="alert alert-error">Nie udało się wczytać galerii.</div>
        }
        @default {
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
            @for (asset of pager.items(); track asset.assetId) {
              <app-asset-card [asset]="asset">
                @if (canDownload) {
                  <button
                    class="btn btn-sm btn-primary"
                    type="button"
                    [disabled]="downloads.pending() === asset.assetId"
                    (click)="downloads.start(asset.assetId)"
                  >
                    Pobierz
                  </button>
                }
                @if (canEdit) {
                  <button class="btn btn-sm" type="button" (click)="editing.set(asset)">
                    Opisz
                  </button>
                }
                @if (canDelete) {
                  <button
                    class="btn btn-sm btn-error btn-outline"
                    type="button"
                    [disabled]="deletion.pending() === asset.assetId"
                    (click)="deletion.remove(asset, () => pager.remove(asset.assetId))"
                  >
                    Usuń
                  </button>
                }
              </app-asset-card>
            } @empty {
              @if (pager.state() === 'ready') {
                <p class="opacity-70">Brak opublikowanych materiałów.</p>
              }
            }
          </div>
          @if (pager.state() === 'loading') {
            <span class="loading loading-spinner mt-4"></span>
          } @else if (pager.nextCursor()) {
            <button class="btn btn-sm mt-4" type="button" (click)="pager.more()">
              Załaduj więcej
            </button>
          }
        }
      }
    }

    @if (editing(); as asset) {
      <app-metadata-editor
        [asset]="asset"
        (saved)="pager.replace($event); editing.set(null)"
        (closed)="editing.set(null)"
      />
    }
  `,
})
export class GalleryPage {
  protected readonly auth = inject(AuthService);
  protected readonly downloads = inject(DownloadService);
  protected readonly deletion = inject(DeleteAssetService);
  private readonly assets = inject(AssetsService);

  protected readonly canBrowse = this.auth.hasAnyGroup(['admin', 'staff', 'viewer']);
  /** UX: pobieranie i tak autoryzuje API (D dostaje 403). */
  protected readonly canDownload = this.auth.hasAnyGroup(['admin', 'staff']);
  protected readonly canDelete = this.auth.hasAnyGroup(['admin']);
  protected readonly canEdit = this.auth.hasAnyGroup(['admin']);
  protected readonly pager = new AssetPager((cursor) => this.assets.list('gallery', cursor));
  /** Asset, którego metadane edytuje A. */
  protected readonly editing = signal<AssetSummary | null>(null);

  constructor() {
    if (this.canBrowse) {
      inject(DictionariesService).load();
      this.pager.reload();
    }
  }
}
