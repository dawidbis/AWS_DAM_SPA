import { TestBed } from '@angular/core/testing';

import { asset } from '../../testing/assets';
import { provideFakeDictionaries } from '../../testing/fake-dictionaries';
import { AssetCard } from './asset-card';

describe('AssetCard', () => {
  const render = async (overrides: Parameters<typeof asset>[1]) => {
    TestBed.configureTestingModule({ imports: [AssetCard], providers: provideFakeDictionaries() });
    const fixture = TestBed.createComponent(AssetCard);
    fixture.componentRef.setInput('asset', asset('a1', overrides));
    await fixture.whenStable();
    return fixture.nativeElement as HTMLElement;
  };

  it('shows category, match, players and tags by name', async () => {
    const element = await render({
      category: 'MATCH_PHOTO',
      matchId: '2025-09-13-unia-lesna',
      seasonId: '2025-26',
      competitionId: 'liga',
      playerIds: ['michal-kruk', 'piotr-zawadzki'],
      tags: ['bramka'],
    });
    const context = element.querySelector('[data-testid="asset-context"]')?.textContent ?? '';
    expect(context).toContain('Zdjęcia meczowe');
    expect(context).toContain('Unia Leśna (dom)');
    expect(element.querySelector('[data-testid="asset-players"]')?.textContent).toContain(
      'Michał Kruk, Piotr Zawadzki',
    );
    expect(element.textContent).toContain('#bramka');
  });

  it('falls back to the identifier for entries removed from dictionaries', async () => {
    const element = await render({ playerIds: ['usuniety-zawodnik'] });
    expect(element.querySelector('[data-testid="asset-players"]')?.textContent).toContain(
      'usuniety-zawodnik',
    );
  });

  it('shows no metadata section for an undescribed asset', async () => {
    const element = await render({});
    expect(element.querySelector('[data-testid="asset-context"]')).toBeNull();
    expect(element.querySelector('[data-testid="asset-players"]')).toBeNull();
  });
});
