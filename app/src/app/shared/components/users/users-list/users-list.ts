import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { LucideDynamicIcon, LucideUserPlus } from '@lucide/angular';
import { UserCard } from '@components/users/user-card/user-card';
import { GroupDialog } from '@components/chats/group-dialog/group-dialog';
import { Member, ROLE_ADMIN, ROLE_MODERATOR } from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { ModalService } from '@services/modal.service';
import { CommunityStore } from '@stores/community.store';
import { ConnectedUserStore } from '@stores/connected-user.store';
import { DmStore } from '@stores/dm.store';

/** Members panel: the open community's members by role, or the open conversation's people. */
@Component({
  selector: 'app-users-list',
  imports: [UserCard, LucideDynamicIcon],
  templateUrl: './users-list.html',
  styleUrl: './users-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class UsersList {
  private readonly account = inject(ConnectedUserStore);
  private readonly modalService = inject(ModalService);
  protected readonly dm = inject(DmStore);
  protected readonly community = inject(CommunityStore);
  protected readonly addPeopleIcon = LucideUserPlus;

  /** Admins, moderators, then the rest, as sections. Only in the server view. */
  public readonly sections = computed(() => {
    const members = this.community.channel() === null ? [] : this.community.members();
    const admins = members.filter((m) => this.isAdmin(m));
    const moderators = members.filter((m) => !this.isAdmin(m) && this.isModerator(m));
    const others = members.filter((m) => !this.isAdmin(m) && !this.isModerator(m));
    return [
      { role: ROLE_ADMIN, members: admins },
      { role: ROLE_MODERATOR, members: moderators },
      { role: 'member', members: others },
    ].filter((s) => s.members.length);
  });

  /** The open conversation's people (DM view only). */
  public readonly roomMembers = computed(() =>
    this.community.channel() === null ? this.dm.room() : [],
  );

  protected readonly me = computed(() => this.account.user()?.pubkey);

  public cardOf(pubkey: string): UserShort {
    return {
      id: pubkey,
      username: this.dm.label(pubkey),
      picture: this.dm.peers()[pubkey]?.picture,
    };
  }

  public rename(input: HTMLInputElement): void {
    void this.dm.rename(input.value);
  }

  /** A new group, starting from these members: membership never changes (spec § 5). */
  public addPeople(): void {
    this.dm.setGroupDraft(this.dm.others(this.dm.selected() ?? ''));
    this.modalService.open(GroupDialog);
  }

  public card(member: Member): UserShort {
    return this.cardOf(member.pubkey);
  }

  public isMe(member: Member): boolean {
    return member.pubkey === this.account.user()?.pubkey;
  }

  public isAdmin(member: Member): boolean {
    return member.roles.includes(ROLE_ADMIN);
  }

  public isModerator(member: Member): boolean {
    return member.roles.includes(ROLE_MODERATOR);
  }

  /** Only an admin names or unnames moderators, never another admin. */
  public canPromote(member: Member): boolean {
    return this.community.isAdmin() && !this.isMe(member) && !this.isAdmin(member);
  }

  /** Admins remove anyone; moderators everyone but admins (the relay checks too). */
  public canRemove(member: Member): boolean {
    return (
      !this.isMe(member) &&
      (this.community.isAdmin() || (!!this.community.canModerate() && !this.isAdmin(member)))
    );
  }

  public toggleModerator(member: Member): void {
    void this.community.setRoles(member.pubkey, this.isModerator(member) ? [] : [ROLE_MODERATOR]);
  }

  public remove(member: Member): void {
    void this.community.removeMember(member.pubkey);
  }
}
