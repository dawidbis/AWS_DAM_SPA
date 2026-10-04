import { HttpErrorResponse } from '@angular/common/http';
import { Component, computed, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { POSITIONS, POSITION_LABELS } from '../../core/api/asset-labels';
import { BROWSER_CONFIRM } from '../../core/api/delete-asset.service';
import { DictionariesService, slugify } from '../../core/api/dictionaries.service';
import { DictionaryKind } from '../../core/api/generated-types/DictionaryKind';

type FieldType = 'text' | 'number' | 'date' | 'checkbox' | 'select';

interface Field {
  key: string;
  label: string;
  type: FieldType;
  /** Opcje listy wyboru (np. sezony dla meczu). */
  options?: () => { value: string; label: string }[];
}

interface KindConfig {
  kind: DictionaryKind;
  label: string;
  /** Pole, z którego generujemy identyfikator nowego wpisu. */
  slugFrom: string;
  fields: Field[];
}

/** Wpis słownika w formularzu: wartości jako tekst/flagi, `id` osobno. */
type Draft = Record<string, string | boolean>;

/**
 * Słowniki klubu (A): zawodnicy, sezony, rozgrywki, mecze, sponsorzy.
 * Formularz jest wspólny, a pola wynikają z konfiguracji rodzaju. Walidację
 * (slug, długości, daty, istnienie sezonu i rozgrywek meczu) robi API.
 */
@Component({
  selector: 'app-dictionaries-page',
  imports: [RouterLink],
  template: `
    <div class="mb-4 flex items-center justify-between gap-2">
      <h2 class="text-2xl font-bold">Słowniki</h2>
      <a class="btn btn-sm btn-ghost" routerLink="/admin">← Administracja</a>
    </div>
    <p class="mb-4 text-sm opacity-80">
      Metadane assetów odwołują się do tych wpisów, dzięki czemu galerię można filtrować. Usunięty
      wpis zostaje w opisach assetów jako identyfikator, który można poprawić.
    </p>

    <div role="tablist" class="tabs tabs-border mb-4">
      @for (config of configs; track config.kind) {
        <button
          role="tab"
          type="button"
          class="tab"
          [class.tab-active]="config.kind === active().kind"
          [attr.data-testid]="'tab-' + config.kind"
          (click)="select(config)"
        >
          {{ config.label }} ({{ count(config.kind) }})
        </button>
      }
    </div>

    @if (message(); as message) {
      <div class="alert mb-4" [class.alert-success]="message.ok" [class.alert-error]="!message.ok">
        {{ message.text }}
      </div>
    }
    @if (dictionaries.state() === 'error') {
      <div class="alert alert-error mb-4">Nie udało się wczytać słowników.</div>
    }

    <div class="overflow-x-auto">
      <table class="table table-sm">
        <thead>
          <tr>
            <th>Identyfikator</th>
            @for (field of active().fields; track field.key) {
              <th>{{ field.label }}</th>
            }
            <th></th>
          </tr>
        </thead>
        <tbody>
          @for (entry of entries(); track entry['id']) {
            <tr [attr.data-testid]="'entry-' + entry['id']">
              <td class="font-mono text-xs">{{ entry['id'] }}</td>
              @for (field of active().fields; track field.key) {
                <td>{{ display(field, entry[field.key]) }}</td>
              }
              <td class="flex justify-end gap-1">
                <button class="btn btn-xs" type="button" (click)="edit(entry)">Edytuj</button>
                <button
                  class="btn btn-xs btn-error btn-outline"
                  type="button"
                  (click)="remove(String(entry['id']))"
                >
                  Usuń
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td [attr.colspan]="active().fields.length + 2" class="opacity-70">Brak wpisów.</td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <section class="card bg-base-200 mt-6 max-w-2xl">
      <div class="card-body gap-3">
        <h3 class="card-title text-base">
          {{
            editingId() ? 'Edycja: ' + editingId() : 'Nowy wpis: ' + active().label.toLowerCase()
          }}
        </h3>
        @for (field of active().fields; track field.key) {
          @switch (field.type) {
            @case ('checkbox') {
              <label class="label cursor-pointer justify-start gap-2">
                <input
                  type="checkbox"
                  class="checkbox checkbox-sm"
                  [attr.data-testid]="'field-' + field.key"
                  [checked]="!!draft()[field.key]"
                  (change)="set(field.key, $any($event.target).checked)"
                />
                {{ field.label }}
              </label>
            }
            @case ('select') {
              <label class="form-control">
                <span class="label-text">{{ field.label }}</span>
                <select
                  class="select select-bordered w-full"
                  [attr.data-testid]="'field-' + field.key"
                  (change)="set(field.key, $any($event.target).value)"
                >
                  <option value="" [selected]="!draft()[field.key]">— wybierz —</option>
                  @for (option of field.options?.() ?? []; track option.value) {
                    <option [value]="option.value" [selected]="draft()[field.key] === option.value">
                      {{ option.label }}
                    </option>
                  }
                </select>
              </label>
            }
            @default {
              <label class="form-control">
                <span class="label-text">{{ field.label }}</span>
                <input
                  class="input input-bordered w-full"
                  [type]="field.type"
                  [attr.data-testid]="'field-' + field.key"
                  [value]="draft()[field.key] ?? ''"
                  (input)="set(field.key, $any($event.target).value)"
                />
              </label>
            }
          }
        }
        <label class="form-control">
          <span class="label-text">Identyfikator (małe litery, cyfry, myślniki)</span>
          <input
            class="input input-bordered w-full font-mono"
            data-testid="field-id"
            [value]="id()"
            [disabled]="!!editingId()"
            (input)="setId($any($event.target).value)"
          />
        </label>
        <div class="card-actions justify-end">
          @if (editingId()) {
            <button class="btn btn-sm" type="button" (click)="reset()">Anuluj</button>
          }
          <button
            class="btn btn-sm btn-primary"
            type="button"
            data-testid="save-entry"
            [disabled]="saving() || !id()"
            (click)="save()"
          >
            Zapisz
          </button>
        </div>
      </div>
    </section>
  `,
})
export class DictionariesPage {
  protected readonly dictionaries = inject(DictionariesService);
  private readonly confirm = inject(BROWSER_CONFIRM);
  protected readonly String = String;

  protected readonly configs: KindConfig[] = [
    {
      kind: 'players',
      label: 'Zawodnicy',
      slugFrom: 'name',
      fields: [
        { key: 'name', label: 'Imię i nazwisko', type: 'text' },
        { key: 'number', label: 'Numer', type: 'number' },
        {
          key: 'position',
          label: 'Pozycja',
          type: 'select',
          options: () => POSITIONS.map((value) => ({ value, label: POSITION_LABELS[value] })),
        },
        { key: 'active', label: 'W obecnej kadrze', type: 'checkbox' },
      ],
    },
    {
      kind: 'seasons',
      label: 'Sezony',
      slugFrom: 'name',
      fields: [{ key: 'name', label: 'Nazwa (np. 2025/26)', type: 'text' }],
    },
    {
      kind: 'competitions',
      label: 'Rozgrywki',
      slugFrom: 'name',
      fields: [{ key: 'name', label: 'Nazwa', type: 'text' }],
    },
    {
      kind: 'matches',
      label: 'Mecze',
      slugFrom: 'opponent',
      fields: [
        { key: 'date', label: 'Data', type: 'date' },
        { key: 'opponent', label: 'Przeciwnik', type: 'text' },
        {
          key: 'seasonId',
          label: 'Sezon',
          type: 'select',
          options: () =>
            this.dictionaries.data().seasons.map((s) => ({ value: s.id, label: s.name })),
        },
        {
          key: 'competitionId',
          label: 'Rozgrywki',
          type: 'select',
          options: () =>
            this.dictionaries.data().competitions.map((c) => ({ value: c.id, label: c.name })),
        },
        { key: 'home', label: 'U siebie', type: 'checkbox' },
      ],
    },
    {
      kind: 'sponsors',
      label: 'Sponsorzy',
      slugFrom: 'name',
      fields: [{ key: 'name', label: 'Nazwa', type: 'text' }],
    },
  ];

  protected readonly active = signal<KindConfig>(this.configs[0]);
  protected readonly draft = signal<Draft>({});
  protected readonly id = signal('');
  /** Identyfikator edytowanego wpisu (`null` = nowy wpis). */
  protected readonly editingId = signal<string | null>(null);
  /** Czy A wpisał identyfikator ręcznie (wtedy nie generujemy go z nazwy). */
  private readonly idTouched = signal(false);
  protected readonly saving = signal(false);
  protected readonly message = signal<{ ok: boolean; text: string } | null>(null);

  protected readonly entries = computed(
    () => this.dictionaries.data()[this.active().kind] as unknown as Record<string, unknown>[],
  );

  constructor() {
    this.dictionaries.load(true);
    this.reset();
  }

  protected count(kind: DictionaryKind): number {
    return this.dictionaries.data()[kind].length;
  }

  protected select(config: KindConfig): void {
    this.active.set(config);
    this.message.set(null);
    this.reset();
  }

  protected display(field: Field, value: unknown): string {
    if (field.type === 'checkbox') {
      return value ? 'tak' : 'nie';
    }
    if (field.type === 'select' && typeof value === 'string') {
      return field.options?.().find((option) => option.value === value)?.label ?? value;
    }
    return value === null || value === undefined ? '' : String(value);
  }

  protected set(key: string, value: string | boolean): void {
    this.draft.update((draft) => ({ ...draft, [key]: value }));
    if (key === this.active().slugFrom && !this.idTouched() && !this.editingId()) {
      const prefix = this.active().kind === 'matches' ? `${this.draft()['date'] ?? ''}-` : '';
      this.id.set(slugify(`${prefix}${value}`));
    }
  }

  protected setId(value: string): void {
    this.idTouched.set(true);
    this.id.set(value.trim());
  }

  protected edit(entry: Record<string, unknown>): void {
    const draft: Draft = {};
    for (const field of this.active().fields) {
      const value = entry[field.key];
      draft[field.key] =
        field.type === 'checkbox' ? Boolean(value) : value == null ? '' : String(value);
    }
    this.draft.set(draft);
    this.id.set(String(entry['id']));
    this.editingId.set(String(entry['id']));
    this.message.set(null);
  }

  protected reset(): void {
    const defaults: Draft = {};
    for (const field of this.active().fields) {
      defaults[field.key] = field.type === 'checkbox' ? field.key === 'active' : '';
    }
    this.draft.set(defaults);
    this.id.set('');
    this.idTouched.set(false);
    this.editingId.set(null);
  }

  /** Ciało `PUT`: liczby jako liczby, puste pola opcjonalne jako `null`. */
  private body(): Record<string, unknown> {
    const body: Record<string, unknown> = {};
    for (const field of this.active().fields) {
      const value = this.draft()[field.key];
      if (field.type === 'checkbox') {
        body[field.key] = Boolean(value);
      } else if (field.type === 'number') {
        body[field.key] = value === '' ? null : Number(value);
      } else if (field.key === 'position') {
        body[field.key] = value === '' ? null : value;
      } else {
        body[field.key] = value;
      }
    }
    return body;
  }

  protected save(): void {
    const kind = this.active().kind;
    const id = this.id();
    this.saving.set(true);
    this.message.set(null);
    this.dictionaries.save(kind, id, this.body()).subscribe({
      next: () => {
        this.saving.set(false);
        this.message.set({ ok: true, text: `Zapisano ${id}.` });
        this.reset();
      },
      error: (error: unknown) => {
        this.saving.set(false);
        this.message.set({ ok: false, text: apiMessage(error, 'Nie udało się zapisać wpisu.') });
      },
    });
  }

  protected remove(id: string): void {
    if (!this.confirm(`Usunąć wpis „${id}”?`)) {
      return;
    }
    this.dictionaries.remove(this.active().kind, id).subscribe({
      next: () => this.message.set({ ok: true, text: `Usunięto ${id}.` }),
      error: (error: unknown) =>
        this.message.set({ ok: false, text: apiMessage(error, 'Nie udało się usunąć wpisu.') }),
    });
  }
}

/** Komunikat błędu z API (`{ message }`) albo tekst domyślny. */
function apiMessage(error: unknown, fallback: string): string {
  return error instanceof HttpErrorResponse && typeof error.error?.message === 'string'
    ? error.error.message
    : fallback;
}
