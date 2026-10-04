import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { of, throwError } from 'rxjs';

import { AssetsService } from '../../core/api/assets.service';
import { asset } from '../../testing/assets';
import { provideFakeDictionaries } from '../../testing/fake-dictionaries';
import { AdminPage } from './admin-page';

describe('AdminPage', () => {
  const setup = async (
    publish: AssetsService['publish'],
    rescan: AssetsService['rescan'] = () => of(),
  ) => {
    const assets = {
      list: vi.fn((view: string) =>
        view === 'failed'
          ? of({ items: [asset('f1', { status: 'SCAN_FAILED' })], nextCursor: null })
          : of({ items: [asset('a1', { status: 'CLEAN_DRAFT' }), asset('a2')], nextCursor: null }),
      ),
      publish: vi.fn(publish),
      rescan: vi.fn(rescan),
      downloadUrl: vi.fn(),
    };
    TestBed.configureTestingModule({
      imports: [AdminPage],
      providers: [
        { provide: AssetsService, useValue: assets },
        provideRouter([]),
        ...provideFakeDictionaries(),
      ],
    });
    const fixture = TestBed.createComponent(AdminPage);
    await fixture.whenStable();
    return { fixture, assets, element: fixture.nativeElement as HTMLElement };
  };

  interface Fixture {
    whenStable(): Promise<unknown>;
  }
  const click = async (label: string, fixture: Fixture, element: HTMLElement) => {
    const button = Array.from(element.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === label,
    );
    button?.click();
    await fixture.whenStable();
  };
  const publishFirst = (fixture: Fixture, element: HTMLElement) =>
    click('Publikuj', fixture, element);
  const cards = (element: HTMLElement) => element.querySelectorAll('[data-testid="asset-card"]');

  it('loads the publication queue and failed scans', async () => {
    const { assets, element } = await setup(() => of());
    expect(assets.list).toHaveBeenCalledWith('drafts', null);
    expect(assets.list).toHaveBeenCalledWith('failed', null);
    expect(cards(element)).toHaveLength(3);
    expect(element.textContent).toContain('Błąd skanu');
  });

  it('restarts a failed scan and removes it from the list', async () => {
    const { fixture, assets, element } = await setup(
      () => of(),
      (id) => of({ assetId: id, status: 'SCANNING' }),
    );
    await click('Skanuj ponownie', fixture, element);
    expect(assets.rescan).toHaveBeenCalledWith('f1');
    expect(cards(element)).toHaveLength(2);
    expect(element.textContent).toContain('Skan uruchomiony ponownie');
  });

  it('removes a published asset from the queue', async () => {
    const { fixture, assets, element } = await setup((id) =>
      of({ assetId: id, status: 'PUBLISHED' }),
    );
    await publishFirst(fixture, element);
    expect(assets.publish).toHaveBeenCalledWith('a1');
    expect(cards(element)).toHaveLength(2);
    expect(element.textContent).toContain('Opublikowano');
  });

  it('keeps the asset and shows an error when publishing fails', async () => {
    const { fixture, element } = await setup(() => throwError(() => new Error('409')));
    await publishFirst(fixture, element);
    expect(cards(element)).toHaveLength(3);
    expect(element.textContent).toContain('Nie udało się opublikować');
  });
});
