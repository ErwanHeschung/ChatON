import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import {
  FormControl,
  FormGroup,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms';
import { Router } from '@angular/router';
import { CmdError, ConnectedUser } from '@models/Auth.model';
import { AuthService } from '@services/auth.service';
import { LucideCircleExclamationPoint, LucideDynamicIcon } from '@lucide/angular';
import { finalize } from 'rxjs';
import { ROUTES } from '@app/app.routes';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { ModalService } from '@services/modal.service';

interface LinkJoinForm {
  handle: FormControl<string>;
  code: FormControl<string>;
}

/** New device: handle + the code shown on a signed-in device (spec/protocol.md § 8). */
@Component({
  selector: 'app-link-join',
  imports: [ReactiveFormsModule, LucideDynamicIcon],
  templateUrl: './link-join.html',
  styleUrl: '../login/login.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LinkJoin {
  private readonly fb = inject(NonNullableFormBuilder);
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);
  private readonly connectedUserStore = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);

  public isPending = signal<boolean>(false);
  public errorIcon = LucideCircleExclamationPoint;

  public readonly form: FormGroup<LinkJoinForm> = this.fb.group({
    handle: ['', [Validators.required, Validators.pattern(/^@?[^@\s]+@[^@\s]+$/)]],
    // 10 chars once dashes and spaces are dropped; core does the real parsing.
    code: ['', [Validators.required, Validators.pattern(/^[\s-]*([0-9a-zA-Z][\s-]*){10}$/)]],
  });

  public onSubmit(): void {
    if (this.form.invalid) {
      this.form.markAllAsTouched();
      return;
    }
    this.isPending.set(true);
    this.authService
      .linkJoin(this.form.getRawValue())
      .pipe(finalize(() => this.isPending.set(false)))
      .subscribe({
        next: (user: ConnectedUser) => {
          this.connectedUserStore.setLoggedInUser(user);
          this.modalService.close();
          this.router.navigate([ROUTES.CHAT, ROUTES.DMS]);
        },
        error: (err: CmdError) => {
          const known = ['not_found', 'wrong_code', 'code_expired', 'link_refused', 'keychain'];
          this.form.setErrors({ [known.includes(err.code) ? err.code : 'unknown']: true });
        },
      });
  }
}
