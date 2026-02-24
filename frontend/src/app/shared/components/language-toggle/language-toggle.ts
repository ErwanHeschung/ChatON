import { ChangeDetectionStrategy, Component, inject, LOCALE_ID } from '@angular/core';

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
    const { pathname, search, hash } = globalThis.location;

    if (newLang !== this.activeLocale) {
      let newPath: string;

      if (pathname.startsWith(`/${this.activeLocale}/`)) {
        newPath = pathname.replace(`/${this.activeLocale}/`, `/${newLang}/`);
      } else {
        newPath = `/${newLang}${pathname}`;
      }

      globalThis.localStorage.setItem('user-lang', newLang);
      globalThis.location.href = `${newPath}${search}${hash}`;
    }
  }
}
