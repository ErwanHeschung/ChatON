import { NgOptimizedImage } from '@angular/common';
import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { LinkJoin } from '@components/auth/link-join/link-join';
import { Login } from '@components/auth/login/login';
import { Register } from '@components/auth/register/register';
import { LanguageToggle } from '@components/language-toggle/language-toggle';
import { PublicLayout } from '@layouts/public/public.layout';
import { ModalService } from '@services/modal.service';

@Component({
  selector: 'app-landing-page',
  imports: [NgOptimizedImage, LanguageToggle, PublicLayout],
  templateUrl: './landing.page.html',
  styleUrl: './landing.page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LandingPage {
  private readonly modalService = inject(ModalService);

  public openLogin(): void {
    this.modalService.open(Login);
  }

  public openLinkJoin(): void {
    this.modalService.open(LinkJoin);
  }

  public openRegister(): void {
    this.modalService.open(Register);
  }
}
