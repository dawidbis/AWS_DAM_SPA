import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { of } from 'rxjs';

import { AssetsService } from '../../core/api/assets.service';
import { AssetStatus } from '../../core/api/generated-types/AssetStatus';
import { asset } from '../../testing/assets';
import { MySubmissionsPage, SUBMISSIONS_POLL_MS } from './my-submissions-page';

describe('MySubmissionsPage', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const setup = async (statuses: AssetStatus[][]) => {
    let call = 0;
    const assets = {
      list: vi.fn(() => {
        const page = statuses[Math.min(call++, statuses.length - 1)];
        return of({
          items: page.map((status, i) => asset(`a${i}`, { status })),
          nextCursor: null,
        });
      }),
    };
    TestBed.configureTestingModule({
      imports: [MySubmissionsPage],
      providers: [provideRouter([]), { provide: AssetsService, useValue: assets }],
    });
    const fixture = TestBed.createComponent(MySubmissionsPage);
    fixture.detectChanges();
    return { fixture, assets, element: fixture.nativeElement as HTMLElement };
  };

  it('shows statuses in Polish', async () => {
    const { element, assets } = await setup([['CLEAN_DRAFT', 'REJECTED']]);
    expect(assets.list).toHaveBeenCalledWith('mine', null);
    const rows = element.querySelectorAll('[data-testid="submission-row"]');
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain('Czeka na publikację');
    expect(rows[1].textContent).toContain('Odrzucony');
  });

  it('polls while a file is being scanned and stops afterwards', async () => {
    const { fixture, assets } = await setup([['SCANNING'], ['CLEAN_DRAFT']]);
    expect(assets.list).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(SUBMISSIONS_POLL_MS);
    fixture.detectChanges();
    expect(assets.list).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(SUBMISSIONS_POLL_MS * 3);
    fixture.detectChanges();
    expect(assets.list).toHaveBeenCalledTimes(2);
  });
});
