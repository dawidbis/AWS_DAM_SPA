import { DatePipe } from '@angular/common';
import { HttpClient } from '@angular/common/http';
import { Injectable, LOCALE_ID, computed, inject, signal } from '@angular/core';
import { Observable, tap } from 'rxjs';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { Dictionaries } from './generated-types/Dictionaries';
import { DictionaryKind } from './generated-types/DictionaryKind';
import { Match } from './generated-types/Match';

export type DictionariesState = 'idle' | 'loading' | 'ready' | 'error';

const EMPTY: Dictionaries = {
  players: [],
  seasons: [],
  competitions: [],
  matches: [],
  sponsors: [],
};

/**
 * Słowniki klubu (`GET /dictionaries`) trzymane w pamięci: kafelki assetów
 * wyświetlają nazwy zamiast identyfikatorów, formularze mają listy wyboru.
 * Słowniki są małe, więc ładujemy je raz i odświeżamy po edycji.
 */
@Injectable({ providedIn: 'root' })
export class DictionariesService {
  private readonly http = inject(HttpClient);
  private readonly apiUrl = inject(RUNTIME_CONFIG).apiUrl;
  private readonly date = new DatePipe(inject(LOCALE_ID));

  readonly state = signal<DictionariesState>('idle');
  readonly data = signal<Dictionaries>(EMPTY);

  private readonly players = computed(() => new Map(this.data().players.map((p) => [p.id, p])));
  private readonly seasons = computed(() => new Map(this.data().seasons.map((s) => [s.id, s])));
  private readonly competitions = computed(
    () => new Map(this.data().competitions.map((c) => [c.id, c])),
  );
  private readonly matches = computed(() => new Map(this.data().matches.map((m) => [m.id, m])));

  /** Wczytuje słowniki, jeśli jeszcze ich nie ma (albo zawsze z `force`). */
  load(force = false): void {
    if (!force && (this.state() === 'ready' || this.state() === 'loading')) {
      return;
    }
    this.state.set('loading');
    this.http.get<Dictionaries>(`${this.apiUrl}/dictionaries`).subscribe({
      next: (data) => {
        this.data.set(data);
        this.state.set('ready');
      },
      error: () => this.state.set('error'),
    });
  }

  /** Utworzenie lub zastąpienie wpisu (A); po sukcesie słowniki są odświeżane. */
  save(kind: DictionaryKind, id: string, body: Record<string, unknown>): Observable<unknown> {
    return this.http
      .put(`${this.apiUrl}/dictionaries/${kind}/${encodeURIComponent(id)}`, body)
      .pipe(tap(() => this.load(true)));
  }

  remove(kind: DictionaryKind, id: string): Observable<unknown> {
    return this.http
      .delete(`${this.apiUrl}/dictionaries/${kind}/${encodeURIComponent(id)}`)
      .pipe(tap(() => this.load(true)));
  }

  // Etykiety do wyświetlania. Wpis usunięty ze słownika pokazujemy jako jego
  // identyfikator, żeby A widział, że metadane trzeba poprawić.

  playerName(id: string): string {
    return this.players().get(id)?.name ?? id;
  }

  seasonName(id: string): string {
    return this.seasons().get(id)?.name ?? id;
  }

  competitionName(id: string): string {
    return this.competitions().get(id)?.name ?? id;
  }

  match(id: string): Match | undefined {
    return this.matches().get(id);
  }

  /** Np. „Unia Leśna (dom) · 13.09.2025”. */
  matchLabel(id: string): string {
    const game = this.match(id);
    if (!game) {
      return id;
    }
    const venue = game.home ? 'dom' : 'wyjazd';
    return `${game.opponent} (${venue}) · ${this.date.transform(game.date, 'shortDate')}`;
  }
}

/**
 * Identyfikator (slug) z nazwy: „Łukasz Sokół” → „lukasz-sokol”. Ten sam
 * format sprawdza backend (`shared::dictionary::is_slug`).
 */
export function slugify(text: string): string {
  return (
    text
      .toLowerCase()
      .replaceAll('ł', 'l')
      // Rozkład na litery i znaki diakrytyczne (NFD), potem usunięcie znaków (\p{M}).
      .normalize('NFD')
      .replaceAll(/\p{M}/gu, '')
      .replaceAll(/[^a-z0-9]+/g, '-')
      .replaceAll(/^-+|-+$/g, '')
      .slice(0, 64)
  );
}
