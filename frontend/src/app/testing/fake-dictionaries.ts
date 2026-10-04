import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';

import { DictionariesService } from '../core/api/dictionaries.service';
import { Dictionaries } from '../core/api/generated-types/Dictionaries';
import { RUNTIME_CONFIG } from '../core/config/runtime-config';
import { TEST_RUNTIME_CONFIG } from './fake-oidc';

/** Przykładowe słowniki do testów (podzbiór seeda z scripts/seed). */
export const TEST_DICTIONARIES: Dictionaries = {
  players: [
    {
      id: 'piotr-zawadzki',
      name: 'Piotr Zawadzki',
      number: 1,
      position: 'GOALKEEPER',
      active: true,
    },
    { id: 'michal-kruk', name: 'Michał Kruk', number: 9, position: 'FORWARD', active: true },
    { id: 'rafal-czapla', name: 'Rafał Czapla', number: 23, position: 'FORWARD', active: false },
  ],
  seasons: [
    { id: '2025-26', name: '2025/26' },
    { id: '2024-25', name: '2024/25' },
  ],
  competitions: [
    { id: 'liga', name: 'Liga Regionalna' },
    { id: 'puchar', name: 'Puchar Regionu' },
  ],
  matches: [
    {
      id: '2025-09-13-unia-lesna',
      seasonId: '2025-26',
      competitionId: 'liga',
      opponent: 'Unia Leśna',
      date: '2025-09-13',
      home: true,
    },
  ],
  sponsors: [{ id: 'bank-matchday', name: 'Bank Matchday' }],
};

/**
 * Prawdziwy DictionariesService z gotowymi danymi (bez żądania do API) oraz
 * HttpClient testowy: komponenty mogą wywoływać `load()` bez efektów, a testy
 * zapisu sprawdzają żądania przez HttpTestingController.
 */
export function provideFakeDictionaries(data: Dictionaries = TEST_DICTIONARIES) {
  return [
    provideHttpClient(),
    provideHttpClientTesting(),
    { provide: RUNTIME_CONFIG, useValue: { ...TEST_RUNTIME_CONFIG, apiUrl: 'https://api.test' } },
    {
      provide: DictionariesService,
      useFactory: () => {
        const service = new DictionariesService();
        service.data.set(data);
        service.state.set('ready');
        return service;
      },
    },
  ];
}
