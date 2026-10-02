import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { PartTarget } from './upload-protocol';

export interface InitUploadResponse {
  assetId: string;
  partSize: number;
  partCount: number;
  parts: PartTarget[];
  urlsExpireInSeconds: number;
}

export interface UploadStatusResponse extends InitUploadResponse {
  status: string;
  uploadedParts: number[];
}

export interface CompleteUploadResponse {
  assetId: string;
  status: string;
}

/** Endpointy uploadu. Token JWT dokleja interceptor (tylko dla apiUrl). */
@Injectable({ providedIn: 'root' })
export class UploadApiService {
  private readonly http = inject(HttpClient);
  private readonly apiUrl = inject(RUNTIME_CONFIG).apiUrl;

  init(file: File, title: string | null): Observable<InitUploadResponse> {
    return this.http.post<InitUploadResponse>(`${this.apiUrl}/uploads`, {
      filename: file.name,
      size: file.size,
      contentType: file.type,
      ...(title ? { title } : {}),
    });
  }

  status(assetId: string): Observable<UploadStatusResponse> {
    return this.http.get<UploadStatusResponse>(
      `${this.apiUrl}/uploads/${encodeURIComponent(assetId)}`,
    );
  }

  complete(assetId: string): Observable<CompleteUploadResponse> {
    return this.http.post<CompleteUploadResponse>(
      `${this.apiUrl}/uploads/${encodeURIComponent(assetId)}/complete`,
      {},
    );
  }
}
