import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { TEST_RUNTIME_CONFIG } from '../../testing/fake-oidc';
import { AssetPager } from './asset-pager';
import { AssetsService } from './assets.service';
import { asset } from '../../testing/assets';

describe('AssetsService', () => {
  let service: AssetsService;
  let http: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        {
          provide: RUNTIME_CONFIG,
          useValue: { ...TEST_RUNTIME_CONFIG, apiUrl: 'https://api.example' },
        },
      ],
    });
    service = TestBed.inject(AssetsService);
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  it('lists a view with an optional cursor', () => {
    service.list('mine', 'c1').subscribe();
    const request = http.expectOne((r) => r.url === 'https://api.example/assets');
    expect(request.request.params.get('view')).toBe('mine');
    expect(request.request.params.get('cursor')).toBe('c1');
    request.flush({ items: [], nextCursor: null });
  });

  it('requests download links and publishes by asset id', () => {
    service.downloadUrl('a/1').subscribe();
    http.expectOne({ method: 'GET', url: 'https://api.example/assets/a%2F1/download' }).flush({
      url: 'https://s3.example/x',
      expiresInSeconds: 300,
    });

    service.publish('a1').subscribe();
    http.expectOne({ method: 'POST', url: 'https://api.example/assets/a1/publish' }).flush({
      assetId: 'a1',
      status: 'PUBLISHED',
    });
  });

  it('pages through results with AssetPager', () => {
    const pager = new AssetPager((cursor) => service.list('gallery', cursor));

    pager.reload();
    http
      .expectOne((r) => r.url === 'https://api.example/assets' && !r.params.has('cursor'))
      .flush({ items: [asset('a1')], nextCursor: 'next' });
    pager.more();
    http
      .expectOne((r) => r.params.get('cursor') === 'next')
      .flush({ items: [asset('a2')], nextCursor: null });

    expect(pager.items().map((a) => a.assetId)).toEqual(['a1', 'a2']);
    expect(pager.nextCursor()).toBeNull();
    pager.more();
    http.expectNone((r) => r.url === 'https://api.example/assets');

    pager.remove('a1');
    expect(pager.items().map((a) => a.assetId)).toEqual(['a2']);
  });

  it('reports errors', () => {
    const pager = new AssetPager((cursor) => service.list('drafts', cursor));
    pager.reload();
    http
      .expectOne((r) => r.url === 'https://api.example/assets')
      .flush({ message: 'Forbidden' }, { status: 403, statusText: 'Forbidden' });
    expect(pager.state()).toBe('error');
  });
});
