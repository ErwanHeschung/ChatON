import { ChangeDetectionStrategy, Component, inject, OnDestroy, OnInit } from '@angular/core';
import { LucideDynamicIcon, LucidePlus } from '@lucide/angular';
import { UserCard } from '@components/users/user-card/user-card';
import { GroupDialog } from '@components/chats/group-dialog/group-dialog';
import { Conversation, roomKey } from '@models/Dm.model';
import { UserShort } from '@models/User.model';
import { PawTrail } from '@components/paw-trail/paw-trail';
import { ModalService } from '@services/modal.service';
import { ShellService } from '@services/shell.service';
import { DmStore } from '@stores/dm.store';

/** Sidebar of the DM view: find `@pseudo@instance`, list conversations. */
@Component({
  selector: 'app-dm-list',
  imports: [UserCard, LucideDynamicIcon, PawTrail],
  templateUrl: './dm-list.html',
  styleUrl: './dm-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DmList implements OnInit, OnDestroy {
  public readonly dm = inject(DmStore);
  private readonly modal = inject(ModalService);
  private readonly shell = inject(ShellService);
  protected readonly newIcon = LucidePlus;

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
          this.shell.openChat();
        }
      });
    }
  }

  public open(conversation: Conversation): void {
    void this.dm.select(this.key(conversation));
    this.shell.openChat();
  }

  public key(conversation: Conversation): string {
    return roomKey(conversation.room);
  }

  /** A 1:1 shows the other person, a group its name and size. */
  public card(conversation: Conversation): UserShort {
    const key = roomKey(conversation.room);
    const others = this.dm.others(key);
    if (others.length > 1) {
      const title = this.dm.title(key);
      return {
        id: key,
        username: `${title} (${others.length + 1})`,
      };
    }
    const pubkey = others[0] ?? key;
    return {
      id: pubkey,
      username: this.dm.label(pubkey),
      picture: this.dm.peers()[pubkey]?.picture,
    };
  }

  public newGroup(): void {
    this.dm.setGroupDraft([]);
    this.modal.open(GroupDialog);
  }
}
