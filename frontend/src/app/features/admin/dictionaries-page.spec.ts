import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { HttpTestingController } from '@angular/common/http/testing';

import { BROWSER_CONFIRM } from '../../core/api/delete-asset.service';
import { slugify } from '../../core/api/dictionaries.service';
import { TEST_DICTIONARIES, provideFakeDictionaries } from '../../testing/fake-dictionaries';
import { DictionariesPage } from './dictionaries-page';

describe('DictionariesPage', () => {
  const confirm = vi.fn(() => true);

  const setup = async () => {
    TestBed.configureTestingModule({
      imports: [DictionariesPage],
      providers: [
        provideRouter([]),
        ...provideFakeDictionaries(),
        { provide: BROWSER_CONFIRM, useValue: confirm },
      ],
    });
    const fixture = TestBed.createComponent(DictionariesPage);
    const http = TestBed.inject(HttpTestingController);
    // Strona zawsze odświeża słowniki przy wejściu.
    http.expectOne('https://api.test/dictionaries').flush(TEST_DICTIONARIES);
    await fixture.whenStable();
    const element = fixture.nativeElement as HTMLElement;
    const field = <T extends HTMLElement>(id: string) =>
      element.querySelector<T>(`[data-testid="${id}"]`) as T;
    const type = async (id: string, value: string) => {
      const input = field<HTMLInputElement | HTMLSelectElement>(id);
      input.value = value;
      input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? 'change' : 'input'));
      await fixture.whenStable();
    };
    return { fixture, http, element, field, type };
  };

  beforeEach(() => confirm.mockReset().mockReturnValue(true));

  it('lists players and saves a new one with an identifier generated from the name', async () => {
    const { http, element, field, type, fixture } = await setup();
    expect(element.querySelector('[data-testid="entry-michal-kruk"]')?.textContent).toContain(
      'Napastnik',
    );

    await type('field-name', 'Łukasz Sokół');
    await type('field-number', '10');
    expect(field<HTMLInputElement>('field-id').value).toBe('lukasz-sokol');
    field<HTMLButtonElement>('save-entry').click();
    await fixture.whenStable();

    const request = http.expectOne('https://api.test/dictionaries/players/lukasz-sokol');
    expect(request.request.method).toBe('PUT');
    expect(request.request.body).toEqual({
      name: 'Łukasz Sokół',
      number: 10,
      position: null,
      active: true,
    });
    request.flush({ kind: 'players', id: 'lukasz-sokol' });
    http.expectOne('https://api.test/dictionaries').flush(TEST_DICTIONARIES);
    await fixture.whenStable();
    expect(element.textContent).toContain('Zapisano lukasz-sokol.');
  });

  it('shows the API message when a season used by matches cannot be deleted', async () => {
    const { http, element, field, fixture } = await setup();
    field<HTMLButtonElement>('tab-seasons').click();
    await fixture.whenStable();

    const row = element.querySelector('[data-testid="entry-2025-26"]');
    Array.from(row?.querySelectorAll('button') ?? [])
      .find((b) => b.textContent?.trim() === 'Usuń')
      ?.click();
    await fixture.whenStable();

    expect(confirm).toHaveBeenCalledOnce();
    const request = http.expectOne('https://api.test/dictionaries/seasons/2025-26');
    expect(request.request.method).toBe('DELETE');
    request.flush(
      { message: 'Wpis jest używany przez mecze; najpierw je zmień lub usuń' },
      { status: 409, statusText: 'Conflict' },
    );
    await fixture.whenStable();
    expect(element.textContent).toContain('używany przez mecze');
  });

  it('does not delete when the admin cancels', async () => {
    const { http, element, fixture } = await setup();
    confirm.mockReturnValue(false);
    Array.from(element.querySelectorAll('button'))
      .find((b) => b.textContent?.trim() === 'Usuń')
      ?.click();
    await fixture.whenStable();
    http.verify();
  });
});

describe('slugify', () => {
  it('turns Polish names into backend-compatible identifiers', () => {
    expect(slugify('Łukasz Sokół')).toBe('lukasz-sokol');
    expect(slugify('2025-09-13 Unia Leśna!')).toBe('2025-09-13-unia-lesna');
    expect(slugify('  --Żółć--  ')).toBe('zolc');
  });
});
