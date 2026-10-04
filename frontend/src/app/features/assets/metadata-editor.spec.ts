import { HttpErrorResponse } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';

import { AssetsService } from '../../core/api/assets.service';
import { AssetMetadata } from '../../core/api/generated-types/AssetMetadata';
import { AssetSummary } from '../../core/api/generated-types/AssetSummary';
import { asset } from '../../testing/assets';
import { provideFakeDictionaries } from '../../testing/fake-dictionaries';
import { MetadataEditor } from './metadata-editor';

describe('MetadataEditor', () => {
  const setup = async (updateMetadata: AssetsService['updateMetadata']) => {
    const assets = { updateMetadata: vi.fn(updateMetadata) };
    TestBed.configureTestingModule({
      imports: [MetadataEditor],
      providers: [{ provide: AssetsService, useValue: assets }, ...provideFakeDictionaries()],
    });
    const fixture = TestBed.createComponent(MetadataEditor);
    fixture.componentRef.setInput('asset', asset('a1', { title: 'Stary tytuł', tags: ['kibice'] }));
    const saved: AssetSummary[] = [];
    fixture.componentInstance.saved.subscribe((value) => saved.push(value));
    await fixture.whenStable();
    const element = fixture.nativeElement as HTMLElement;
    const field = <T extends HTMLElement>(id: string) =>
      element.querySelector<T>(`[data-testid="${id}"]`) as T;
    const change = async (id: string, value: string) => {
      const input = field<HTMLInputElement | HTMLSelectElement>(id);
      input.value = value;
      input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? 'change' : 'input'));
      await fixture.whenStable();
    };
    return { fixture, assets, element, field, change, saved };
  };

  it('fills season and competition from the selected match and saves normalized metadata', async () => {
    const { assets, field, change, saved, fixture } = await setup((_, metadata) => of(metadata));

    await change('meta-match', '2025-09-13-unia-lesna');
    expect(field<HTMLSelectElement>('meta-season').value).toBe('2025-26');
    expect(field<HTMLSelectElement>('meta-season').disabled).toBe(true);
    expect(field<HTMLSelectElement>('meta-competition').value).toBe('liga');

    await change('meta-category', 'MATCH_PHOTO');
    field<HTMLInputElement>('meta-player-michal-kruk').click();
    await change('meta-tags', 'kibice, bramka , ');
    field<HTMLButtonElement>('meta-save').click();
    await fixture.whenStable();

    const expected: AssetMetadata = {
      title: 'Stary tytuł',
      category: 'MATCH_PHOTO',
      seasonId: '2025-26',
      competitionId: 'liga',
      matchId: '2025-09-13-unia-lesna',
      playerIds: ['michal-kruk'],
      tags: ['kibice', 'bramka'],
    };
    expect(assets.updateMetadata).toHaveBeenCalledWith('a1', expected);
    expect(saved[0]).toMatchObject({ assetId: 'a1', ...expected });
  });

  it('shows the API validation message and keeps the dialog open', async () => {
    const error = new HttpErrorResponse({
      status: 400,
      error: { message: 'Nie ma wpisu players/duch w słownikach' },
    });
    const { field, element, saved, fixture } = await setup(() => throwError(() => error));
    field<HTMLButtonElement>('meta-save').click();
    await fixture.whenStable();
    expect(element.querySelector('[data-testid="meta-error"]')?.textContent).toContain(
      'players/duch',
    );
    expect(saved).toHaveLength(0);
  });
});
