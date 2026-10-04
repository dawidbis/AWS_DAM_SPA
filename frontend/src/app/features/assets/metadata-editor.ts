import { HttpErrorResponse } from '@angular/common/http';
import { Component, OnInit, computed, inject, input, output, signal } from '@angular/core';

import { CATEGORIES, CATEGORY_LABELS } from '../../core/api/asset-labels';
import { AssetsService } from '../../core/api/assets.service';
import { DictionariesService } from '../../core/api/dictionaries.service';
import { AssetCategory } from '../../core/api/generated-types/AssetCategory';
import { AssetMetadata } from '../../core/api/generated-types/AssetMetadata';
import { AssetSummary } from '../../core/api/generated-types/AssetSummary';

/**
 * Edycja metadanych assetu przez A: kategoria, mecz (wyznacza sezon
 * i rozgrywki), zawodnicy, tagi, tytuł. Wartości słowników pochodzą
 * z `GET /dictionaries`, więc nie da się wpisać wolnego tekstu zamiast
 * zawodnika czy meczu. Walidację i tak powtarza API.
 */
@Component({
  selector: 'app-metadata-editor',
  template: `
    <dialog class="modal modal-open" open aria-labelledby="metadata-title">
      <div class="modal-box max-w-2xl">
        <h3 id="metadata-title" class="mb-4 text-lg font-bold">
          Opis: {{ asset().title || asset().originalFilename }}
        </h3>

        <div class="flex flex-col gap-3">
          <label class="form-control">
            <span class="label-text">Tytuł</span>
            <input
              class="input input-bordered w-full"
              maxlength="120"
              data-testid="meta-title"
              [value]="title()"
              (input)="title.set($any($event.target).value)"
            />
          </label>

          <label class="form-control">
            <span class="label-text">Kategoria</span>
            <select
              class="select select-bordered w-full"
              data-testid="meta-category"
              (change)="category.set($any($event.target).value || null)"
            >
              <option value="" [selected]="!category()">— brak —</option>
              @for (value of categoryValues; track value) {
                <option [value]="value" [selected]="category() === value">
                  {{ categoryLabels[value] }}
                </option>
              }
            </select>
          </label>

          <label class="form-control">
            <span class="label-text">Mecz</span>
            <select
              class="select select-bordered w-full"
              data-testid="meta-match"
              (change)="selectMatch($any($event.target).value)"
            >
              <option value="" [selected]="!matchId()">— brak —</option>
              @for (game of dictionaries.data().matches; track game.id) {
                <option [value]="game.id" [selected]="matchId() === game.id">
                  {{ dictionaries.matchLabel(game.id) }} ·
                  {{ dictionaries.competitionName(game.competitionId) }}
                </option>
              }
            </select>
          </label>

          <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <label class="form-control">
              <span class="label-text">Sezon</span>
              <select
                class="select select-bordered w-full"
                data-testid="meta-season"
                [disabled]="!!matchId()"
                (change)="seasonId.set($any($event.target).value || null)"
              >
                <option value="" [selected]="!seasonId()">— brak —</option>
                @for (season of dictionaries.data().seasons; track season.id) {
                  <option [value]="season.id" [selected]="seasonId() === season.id">
                    {{ season.name }}
                  </option>
                }
              </select>
            </label>
            <label class="form-control">
              <span class="label-text">Rozgrywki</span>
              <select
                class="select select-bordered w-full"
                data-testid="meta-competition"
                [disabled]="!!matchId()"
                (change)="competitionId.set($any($event.target).value || null)"
              >
                <option value="" [selected]="!competitionId()">— brak —</option>
                @for (competition of dictionaries.data().competitions; track competition.id) {
                  <option [value]="competition.id" [selected]="competitionId() === competition.id">
                    {{ competition.name }}
                  </option>
                }
              </select>
            </label>
          </div>

          <fieldset class="fieldset">
            <legend class="fieldset-legend">Zawodnicy na zdjęciu</legend>
            <div class="grid max-h-48 grid-cols-1 gap-1 overflow-y-auto sm:grid-cols-2">
              @for (player of dictionaries.data().players; track player.id) {
                <label class="label cursor-pointer justify-start gap-2">
                  <input
                    type="checkbox"
                    class="checkbox checkbox-sm"
                    [attr.data-testid]="'meta-player-' + player.id"
                    [checked]="playerIds().includes(player.id)"
                    (change)="togglePlayer(player.id, $any($event.target).checked)"
                  />
                  <span>
                    @if (player.number) {
                      {{ player.number }}.
                    }
                    {{ player.name }}
                    @if (!player.active) {
                      <span class="opacity-60">(były)</span>
                    }
                  </span>
                </label>
              }
            </div>
          </fieldset>

          <label class="form-control">
            <span class="label-text">Tagi (oddzielone przecinkami, maks. 10)</span>
            <input
              class="input input-bordered w-full"
              data-testid="meta-tags"
              [value]="tags()"
              (input)="tags.set($any($event.target).value)"
            />
          </label>

          @if (error(); as error) {
            <div class="alert alert-error" data-testid="meta-error">{{ error }}</div>
          }
        </div>

        <div class="modal-action">
          <button class="btn" type="button" [disabled]="saving()" (click)="closed.emit()">
            Anuluj
          </button>
          <button
            class="btn btn-primary"
            type="button"
            data-testid="meta-save"
            [disabled]="saving()"
            (click)="save()"
          >
            Zapisz
          </button>
        </div>
      </div>
    </dialog>
  `,
})
export class MetadataEditor implements OnInit {
  readonly asset = input.required<AssetSummary>();
  /** Asset z zapisanymi metadanymi (do podmiany na liście). */
  readonly saved = output<AssetSummary>();
  readonly closed = output<void>();

  protected readonly dictionaries = inject(DictionariesService);
  private readonly assets = inject(AssetsService);

  protected readonly categoryValues = CATEGORIES;
  protected readonly categoryLabels = CATEGORY_LABELS;

  protected readonly title = signal('');
  protected readonly category = signal<AssetCategory | null>(null);
  protected readonly matchId = signal<string | null>(null);
  protected readonly seasonId = signal<string | null>(null);
  protected readonly competitionId = signal<string | null>(null);
  protected readonly playerIds = signal<string[]>([]);
  protected readonly tags = signal('');
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  private readonly request = computed<AssetMetadata>(() => ({
    title: this.title().trim() || null,
    category: this.category(),
    seasonId: this.seasonId(),
    competitionId: this.competitionId(),
    matchId: this.matchId(),
    playerIds: this.playerIds(),
    tags: this.tags()
      .split(',')
      .map((tag) => tag.trim())
      .filter((tag) => tag.length > 0),
  }));

  constructor() {
    this.dictionaries.load();
  }

  ngOnInit(): void {
    const asset = this.asset();
    this.title.set(asset.title ?? '');
    this.category.set(asset.category);
    this.matchId.set(asset.matchId);
    this.seasonId.set(asset.seasonId);
    this.competitionId.set(asset.competitionId);
    this.playerIds.set([...asset.playerIds]);
    this.tags.set(asset.tags.join(', '));
  }

  /** Mecz wyznacza sezon i rozgrywki (backend sprawdza zgodność). */
  protected selectMatch(id: string): void {
    this.matchId.set(id || null);
    const game = id ? this.dictionaries.match(id) : undefined;
    if (game) {
      this.seasonId.set(game.seasonId);
      this.competitionId.set(game.competitionId);
    }
  }

  protected togglePlayer(id: string, checked: boolean): void {
    this.playerIds.update((ids) =>
      checked ? [...new Set([...ids, id])] : ids.filter((other) => other !== id),
    );
  }

  protected save(): void {
    this.saving.set(true);
    this.error.set(null);
    this.assets.updateMetadata(this.asset().assetId, this.request()).subscribe({
      next: (metadata) => {
        this.saving.set(false);
        this.saved.emit({ ...this.asset(), ...metadata });
      },
      error: (error: unknown) => {
        this.saving.set(false);
        const message =
          error instanceof HttpErrorResponse && typeof error.error?.message === 'string'
            ? error.error.message
            : null;
        this.error.set(message ?? 'Nie udało się zapisać metadanych. Spróbuj ponownie.');
      },
    });
  }
}
