import { Component, input } from '@angular/core';

/** Tymczasowa strona sekcji, której funkcje powstają w kolejnych krokach etapu 1. */
@Component({
  selector: 'app-placeholder-page',
  template: `
    <section class="card bg-base-200">
      <div class="card-body">
        <h2 class="card-title">{{ title() }}</h2>
        <p>{{ description() }}</p>
      </div>
    </section>
  `,
})
export class PlaceholderPage {
  readonly title = input.required<string>();
  readonly description = input.required<string>();
}
