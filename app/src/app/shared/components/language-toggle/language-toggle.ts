import { ChangeDetectionStrategy, Component, inject, LOCALE_ID } from '@angular/core';

export const LANG_KEY = 'user-lang';

@Component({
  selector: 'app-language-toggle',
  imports: [],
  templateUrl: './language-toggle.html',
  styleUrl: './language-toggle.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LanguageToggle {
  public activeLocale: string = inject(LOCALE_ID);

  switchLanguage(newLang: string) {
    if (newLang !== this.activeLocale) {
      globalThis.localStorage.setItem(LANG_KEY, newLang);
      globalThis.location.reload();
    }
  }
}
