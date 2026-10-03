import { HttpClient, HttpParams } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { AssetDeletedResponse } from './generated-types/AssetDeletedResponse';
import { AssetListResponse } from './generated-types/AssetListResponse';
import { AssetView } from './generated-types/AssetView';
import { DownloadResponse } from './generated-types/DownloadResponse';
import { AssetStatusResponse } from './generated-types/AssetStatusResponse';

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

  publish(assetId: string): Observable<AssetStatusResponse> {
    return this.http.post<AssetStatusResponse>(
      `${this.apiUrl}/assets/${encodeURIComponent(assetId)}/publish`,
      {},
    );
  }

  /** Ponowienie skanu po SCAN_FAILED (A). */
  rescan(assetId: string): Observable<AssetStatusResponse> {
    return this.http.post<AssetStatusResponse>(
      `${this.apiUrl}/assets/${encodeURIComponent(assetId)}/rescan`,
      {},
    );
  }

  /** Usunięcie assetu (A). Zainfekowanych i przetwarzanych API nie usunie. */
  remove(assetId: string): Observable<AssetDeletedResponse> {
    return this.http.delete<AssetDeletedResponse>(
      `${this.apiUrl}/assets/${encodeURIComponent(assetId)}`,
    );
  }
}
