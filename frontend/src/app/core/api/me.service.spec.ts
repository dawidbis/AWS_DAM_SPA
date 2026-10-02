import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';

import { RUNTIME_CONFIG } from '../config/runtime-config';
import { TEST_RUNTIME_CONFIG } from '../../testing/fake-oidc';
import { MeService } from './me.service';

describe('MeService', () => {
  const setup = (apiUrl?: string) => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        { provide: RUNTIME_CONFIG, useValue: { ...TEST_RUNTIME_CONFIG, apiUrl } },
      ],
    });
    return { service: TestBed.inject(MeService), http: TestBed.inject(HttpTestingController) };
  };

  it('calls GET {apiUrl}/me', () => {
    const { service, http } = setup('https://api.example');
    let groups: string[] | undefined;

    service.get().subscribe((me) => (groups = me.groups));
    http.expectOne({ method: 'GET', url: 'https://api.example/me' }).flush({
      sub: 'abc',
      email: 'a@example.com',
      groups: ['admin'],
    });

    expect(service.available).toBe(true);
    expect(groups).toEqual(['admin']);
    http.verify();
  });

  it('is unavailable without apiUrl', () => {
    expect(setup().service.available).toBe(false);
  });
});
