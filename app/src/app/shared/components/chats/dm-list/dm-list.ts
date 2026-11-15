import { ChangeDetectionStrategy, Component, inject, OnDestroy, OnInit } from '@angular/core';
import { UserCard } from '@components/users/user-card/user-card';
import { DmStore } from '@stores/dm.store';

/** Left sidebar of the DM view: find `@pseudo@instance`, list conversations. */
@Component({
  selector: 'app-dm-list',
  imports: [UserCard],
  templateUrl: './dm-list.html',
  styleUrl: './dm-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DmList implements OnInit, OnDestroy {
  public readonly dm = inject(DmStore);

  ngOnInit(): void {
    this.dm.load().catch(() => undefined);
  }

  ngOnDestroy(): void {
    void this.dm.select(null);
  }

  public search(input: HTMLInputElement): void {
    const handle = input.value.trim();
    if (handle) {
      void this.dm.search(handle).then(() => {
        if (!this.dm.searchError()) {
          input.value = '';
        }
      });
    }
  }

  public card(pubkey: string) {
    return {
      id: pubkey,
      username: this.dm.label(pubkey),
      profilePicture: '/chatON-logo.svg',
      picture: this.dm.peers()[pubkey]?.picture,
    };
  }
}
