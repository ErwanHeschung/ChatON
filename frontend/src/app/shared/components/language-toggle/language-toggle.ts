import { Component, Inject, LOCALE_ID } from '@angular/core';

@Component({
 selector: 'app-language-toggle',
 imports: [],
 templateUrl: './language-toggle.html',
 styleUrl: './language-toggle.scss',
})
export class LanguageToggle {
 constructor(@Inject(LOCALE_ID) public activeLocale: string) {}

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
