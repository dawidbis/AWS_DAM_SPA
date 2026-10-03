import { AssetSummary } from '../core/api/generated-types/AssetSummary';

/** Przykładowy asset do testów list. */
export function asset(id: string, overrides: Partial<AssetSummary> = {}): AssetSummary {
  return {
    assetId: id,
    status: 'PUBLISHED',
    title: null,
    originalFilename: `${id}.jpg`,
    contentType: 'image/jpeg',
    sizeBytes: 2048,
    createdAt: 1_700_000_000_000,
    updatedAt: 1_700_000_000_000,
    previewUrl: null,
    ...overrides,
  };
}
