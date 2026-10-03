import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';

import { AssetsService } from '../../core/api/assets.service';
import { asset } from '../../testing/assets';
import { AdminPage } from './admin-page';

describe('AdminPage', () => {
  const setup = async (publish: AssetsService['publish']) => {
    const assets = {
      list: vi.fn(() =>
        of({ items: [asset('a1', { status: 'CLEAN_DRAFT' }), asset('a2')], nextCursor: null }),
      ),
      publish: vi.fn(publish),
      downloadUrl: vi.fn(),
    };
    TestBed.configureTestingModule({
      imports: [AdminPage],
      providers: [{ provide: AssetsService, useValue: assets }],
    });
    const fixture = TestBed.createComponent(AdminPage);
    await fixture.whenStable();
    return { fixture, assets, element: fixture.nativeElement as HTMLElement };
  };

  const publishFirst = async (
    fixture: { whenStable(): Promise<unknown> },
    element: HTMLElement,
  ) => {
    const button = Array.from(element.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Publikuj',
    );
    button?.click();
    await fixture.whenStable();
  };

  it('loads the publication queue', async () => {
    const { assets, element } = await setup(() => of());
    expect(assets.list).toHaveBeenCalledWith('drafts', null);
    expect(element.querySelectorAll('[data-testid="asset-card"]')).toHaveLength(2);
  });

  it('removes a published asset from the queue', async () => {
    const { fixture, assets, element } = await setup((id) =>
      of({ assetId: id, status: 'PUBLISHED' }),
    );
    await publishFirst(fixture, element);
    expect(assets.publish).toHaveBeenCalledWith('a1');
    expect(element.querySelectorAll('[data-testid="asset-card"]')).toHaveLength(1);
    expect(element.textContent).toContain('Opublikowano');
  });

  it('keeps the asset and shows an error when publishing fails', async () => {
    const { fixture, element } = await setup(() => throwError(() => new Error('409')));
    await publishFirst(fixture, element);
    expect(element.querySelectorAll('[data-testid="asset-card"]')).toHaveLength(2);
    expect(element.textContent).toContain('Nie udało się opublikować');
  });
});
