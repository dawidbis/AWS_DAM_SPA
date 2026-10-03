import { DatePipe } from '@angular/common';
import { Component, DestroyRef, effect, inject } from '@angular/core';
import { RouterLink } from '@angular/router';

import { AssetPager } from '../../core/api/asset-pager';
import { STATUS_PRESENTATION, isInProgress } from '../../core/api/asset-status';
import { AssetsService } from '../../core/api/assets.service';
import { FileSizePipe } from '../../core/format/file-size.pipe';

/** Co ile odświeżać listę, gdy któryś plik jest jeszcze skanowany. */
export const SUBMISSIONS_POLL_MS = 5000;

/** „Moje zgłoszenia”: statusy plików wgranych przez zalogowanego (A, C). */
@Component({
  selector: 'app-my-submissions-page',
  imports: [DatePipe, FileSizePipe, RouterLink],
  template: `
    <div class="mb-4 flex items-center justify-between gap-2">
      <h2 class="text-2xl font-bold">Moje zgłoszenia</h2>
      <div class="flex gap-2">
        <a class="btn btn-sm btn-primary" routerLink="/upload">+ Foto</a>
        <button class="btn btn-sm" type="button" (click)="pager.reload()">Odśwież</button>
      </div>
    </div>

    @if (pager.state() === 'error') {
      <div class="alert alert-error mb-4">Nie udało się wczytać zgłoszeń.</div>
    }

    <div class="overflow-x-auto">
      <table class="table">
        <thead>
          <tr>
            <th>Plik</th>
            <th>Rozmiar</th>
            <th>Wgrano</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          @for (asset of pager.items(); track asset.assetId) {
            <tr data-testid="submission-row">
              <td class="break-all">
                <div class="font-medium">{{ asset.title || asset.originalFilename }}</div>
                @if (asset.title) {
                  <div class="text-xs opacity-70">{{ asset.originalFilename }}</div>
                }
              </td>
              <td>{{ asset.sizeBytes | fileSize }}</td>
              <td>{{ asset.createdAt | date: 'short' }}</td>
              <td>
                <span class="badge {{ status[asset.status].badge }}">
                  @if (inProgress(asset.status)) {
                    <span class="loading loading-spinner loading-xs"></span>
                  }
                  {{ status[asset.status].label }}
                </span>
              </td>
            </tr>
          } @empty {
            @if (pager.state() === 'ready') {
              <tr>
                <td colspan="4" class="opacity-70">Nie masz jeszcze żadnych zgłoszeń.</td>
              </tr>
            }
          }
        </tbody>
      </table>
    </div>

    @if (pager.state() === 'loading') {
      <span class="loading loading-spinner mt-4"></span>
    } @else if (pager.nextCursor()) {
      <button class="btn btn-sm mt-4" type="button" (click)="pager.more()">Załaduj więcej</button>
    }
  `,
})
export class MySubmissionsPage {
  private readonly assets = inject(AssetsService);

  protected readonly status = STATUS_PRESENTATION;
  protected readonly inProgress = isInProgress;
  protected readonly pager = new AssetPager((cursor) => this.assets.list('mine', cursor));

  constructor() {
    this.pager.reload();

    // Dopóki pipeline skanuje któryś plik, odświeżamy listę.
    let timer: ReturnType<typeof setTimeout> | undefined;
    effect(() => {
      clearTimeout(timer);
      const waiting =
        this.pager.state() === 'ready' &&
        this.pager.items().some((asset) => isInProgress(asset.status));
      if (waiting) {
        timer = setTimeout(() => this.pager.reload(), SUBMISSIONS_POLL_MS);
      }
    });
    inject(DestroyRef).onDestroy(() => clearTimeout(timer));
  }
}
