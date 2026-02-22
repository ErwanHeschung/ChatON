import { NgOptimizedImage } from '@angular/common';
import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { LanguageToggle } from '../../shared/components/language-toggle/language-toggle';
import { Public } from '../../shared/layouts/public/public.layout';
import { ModalService } from '../../shared/modal-service';
import { Register } from '../../shared/components/auth/register/register';
import { Login } from '../../shared/components/auth/login/login';

@Component({
  selector: 'app-landing-page',
  imports: [NgOptimizedImage, LanguageToggle, Public],
  templateUrl: './landing.page.html',
  styleUrl: './landing.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LandingPage {
  private readonly modalService = inject(ModalService);

  public openLogin(): void {
    this.modalService.open(Login);
  }

  public openRegister(): void {
    this.modalService.open(Register);
  }
}
