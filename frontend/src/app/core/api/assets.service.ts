import { HttpClient, HttpParams } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { AssetListResponse } from './generated-types/AssetListResponse';
import { AssetView } from './generated-types/AssetView';
import { DownloadResponse } from './generated-types/DownloadResponse';
import { PublishResponse } from './generated-types/PublishResponse';

/**
 * Katalog assetów. Typy odpowiedzi są generowane z modeli Rusta (ts-rs),
 * więc zmiana kontraktu w Lambdzie psuje build frontendu, a nie produkcję.
 */
@Injectable({ providedIn: 'root' })
export class AssetsService {
  private readonly http = inject(HttpClient);
  private readonly apiUrl = inject(RUNTIME_CONFIG).apiUrl;

  list(view: AssetView, cursor?: string | null): Observable<AssetListResponse> {
    let params = new HttpParams().set('view', view);
    if (cursor) {
      params = params.set('cursor', cursor);
    }
    return this.http.get<AssetListResponse>(`${this.apiUrl}/assets`, { params });
  }

  /** Krótko żyjący link do oryginału (A, B). */
  downloadUrl(assetId: string): Observable<DownloadResponse> {
    return this.http.get<DownloadResponse>(
      `${this.apiUrl}/assets/${encodeURIComponent(assetId)}/download`,
    );
  }

  publish(assetId: string): Observable<PublishResponse> {
    return this.http.post<PublishResponse>(
      `${this.apiUrl}/assets/${encodeURIComponent(assetId)}/publish`,
      {},
    );
  }
}
