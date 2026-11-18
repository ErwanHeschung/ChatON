import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  linkedSignal,
  signal,
} from '@angular/core';
import { AsyncPipe } from '@angular/common';
import { CdkMenu, CdkMenuItemCheckbox, CdkMenuTrigger } from '@angular/cdk/menu';
import {
  LucideArrowDown,
  LucideArrowLeft,
  LucideArrowUp,
  LucideChevronDown,
  LucideCrown,
  LucideDynamicIcon,
  LucideIconInput,
  LucideImage,
  LucidePlus,
  LucideShieldCheck,
  LucideSlidersHorizontal,
  LucideTrash2,
  LucideUsers,
  LucideX,
} from '@lucide/angular';
import { Avatar } from '@components/avatar/avatar';
import { UserCard } from '@components/users/user-card/user-card';
import { CmdError } from '@models/Auth.model';
import { EVERYONE, Member, Permission, ROLE_COLORS, Role } from '@models/Community.model';
import { UserShort } from '@models/User.model';
import { MediaService } from '@services/media.service';
import { ShellService } from '@services/shell.service';
import { CommunityStore } from '@stores/community.store';
import { DmStore } from '@stores/dm.store';
import { ConnectedUserStore } from '@stores/connected-user.store';

type Section = 'overview' | 'roles' | 'members' | 'ownership';
type State = 'idle' | 'saving' | 'done' | CmdError['code'];

/** The permissions a role gives, grouped as the editor shows them (spec § 9). */
const PERMISSION_GROUPS: {
  label: string;
  perms: { id: Permission; label: string; hint: string }[];
}[] = [
  {
    label: $localize`:@@perms.general:Community`,
    perms: [
      {
        id: 'administrator',
        label: $localize`:@@perm.administrator:Administrator`,
        hint: $localize`:@@perm.administrator.hint:Every permission, everywhere. Give it with care.`,
      },
      {
        id: 'manage_community',
        label: $localize`:@@perm.manage_community:Manage the community`,
        hint: $localize`:@@perm.manage_community.hint:Name, picture, description and banner.`,
      },
      {
        id: 'manage_channels',
        label: $localize`:@@perm.manage_channels:Manage channels`,
        hint: $localize`:@@perm.manage_channels.hint:Create, rename, arrange and delete channels.`,
      },
      {
        id: 'manage_roles',
        label: $localize`:@@perm.manage_roles:Manage roles`,
        hint: $localize`:@@perm.manage_roles.hint:Edit and give the roles below their own.`,
      },
      {
        id: 'view_audit_log',
        label: $localize`:@@perm.view_audit_log:View the audit log`,
        hint: $localize`:@@perm.view_audit_log.hint:Read moderation actions and invites.`,
      },
    ],
  },
  {
    label: $localize`:@@perms.members:Members`,
    perms: [
      {
        id: 'create_invite',
        label: $localize`:@@perm.create_invite:Invite`,
        hint: $localize`:@@perm.create_invite.hint:Create invite codes.`,
      },
      {
        id: 'manage_invites',
        label: $localize`:@@perm.manage_invites:Manage invites`,
        hint: $localize`:@@perm.manage_invites.hint:See and revoke everyone's invites.`,
      },
      {
        id: 'kick_members',
        label: $localize`:@@perm.kick_members:Remove members`,
        hint: $localize`:@@perm.kick_members.hint:They can come back with an invite.`,
      },
      {
        id: 'ban_members',
        label: $localize`:@@perm.ban_members:Ban members`,
        hint: $localize`:@@perm.ban_members.hint:They can't come back until unbanned.`,
      },
      {
        id: 'moderate_members',
        label: $localize`:@@perm.moderate_members:Time out members`,
        hint: $localize`:@@perm.moderate_members.hint:Keep them from writing and talking for a while.`,
      },
    ],
  },
  {
    label: $localize`:@@perms.text:Text channels`,
    perms: [
      {
        id: 'view_channel',
        label: $localize`:@@perm.view_channel:See channels`,
        hint: $localize`:@@perm.view_channel.hint:Read channels and their history.`,
      },
      {
        id: 'send_messages',
        label: $localize`:@@perm.send_messages:Send messages`,
        hint: '',
      },
      {
        id: 'attach_files',
        label: $localize`:@@perm.attach_files:Attach files`,
        hint: '',
      },
      {
        id: 'add_reactions',
        label: $localize`:@@perm.add_reactions:Add reactions`,
        hint: '',
      },
      {
        id: 'mention_everyone',
        label: $localize`:@@perm.mention_everyone:Mention everyone`,
        hint: $localize`:@@perm.mention_everyone.hint:@everyone, and roles that can't be mentioned.`,
      },
      {
        id: 'manage_messages',
        label: $localize`:@@perm.manage_messages:Manage messages`,
        hint: $localize`:@@perm.manage_messages.hint:Delete and pin other people's messages.`,
      },
    ],
  },
  {
    label: $localize`:@@perms.voice:Voice channels`,
    perms: [
      { id: 'connect', label: $localize`:@@perm.connect:Connect`, hint: '' },
      { id: 'speak', label: $localize`:@@perm.speak:Speak`, hint: '' },
      { id: 'stream', label: $localize`:@@perm.stream:Share their screen`, hint: '' },
      {
        id: 'mute_members',
        label: $localize`:@@perm.mute_members:Mute members`,
        hint: '',
      },
      {
        id: 'move_members',
        label: $localize`:@@perm.move_members:Move and disconnect members`,
        hint: '',
      },
    ],
  },
];

/** A new role's id: short, random, in the spec's alphabet. */
const roleId = () =>
  Array.from(
    crypto.getRandomValues(new Uint8Array(8)),
    (b) => '0123456789abcdefghjkmnpqrstvwxyz'[b % 32],
  ).join('');

/**
 * Community settings, full screen like the account's: overview, roles, members, ownership.
 * Each section shows only with its permission; the relay checks again (spec § 9).
 */
@Component({
  selector: 'app-server-settings',
  imports: [
    AsyncPipe,
    Avatar,
    CdkMenu,
    CdkMenuItemCheckbox,
    CdkMenuTrigger,
    LucideDynamicIcon,
    UserCard,
  ],
  templateUrl: './server-settings.html',
  styleUrls: ['../../settings/settings.scss', './server-settings.scss'],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    role: 'dialog',
    'aria-modal': 'true',
    'aria-labelledby': 'server-settings-title',
    '[class.list-open]': 'listOpen()',
    // A menu's Escape closes the menu only (CDK prevents its default).
    '(document:keydown.escape)': '$event.defaultPrevented || close()',
  },
})
export class ServerSettings {
  protected readonly store = inject(CommunityStore);
  private readonly shell = inject(ShellService);
  protected readonly dm = inject(DmStore);
  protected readonly media = inject(MediaService);
  private readonly account = inject(ConnectedUserStore);

  protected readonly icons = {
    back: LucideArrowLeft,
    close: LucideX,
    up: LucideArrowUp,
    down: LucideArrowDown,
    plus: LucidePlus,
    trash: LucideTrash2,
    image: LucideImage,
    chevron: LucideChevronDown,
  };
  protected readonly groups = PERMISSION_GROUPS;
  protected readonly colors = ROLE_COLORS;
  protected readonly everyone = EVERYONE;

  protected readonly community = this.store.current;

  protected readonly sections = computed(() => {
    const can = (p: Permission) => this.store.can(p);
    const all: { id: Section; label: string; icon: LucideIconInput; shown: boolean }[] = [
      {
        id: 'overview',
        label: $localize`:@@server-settings.overview:Overview`,
        icon: LucideSlidersHorizontal,
        shown: can('manage_community'),
      },
      {
        id: 'roles',
        label: $localize`:@@server-settings.roles:Roles`,
        icon: LucideShieldCheck,
        shown: can('manage_roles'),
      },
      {
        id: 'members',
        label: $localize`:@@server-settings.members:Members`,
        icon: LucideUsers,
        shown: can('manage_roles') || can('kick_members'),
      },
      {
        id: 'ownership',
        label: $localize`:@@server-settings.ownership:Ownership`,
        icon: LucideCrown,
        shown: this.store.isOwner(),
      },
    ];
    return all.filter((s) => s.shown);
  });
  private readonly picked = signal<Section | null>(null);
  /** The picked section, or the first I may open (permissions can change while open). */
  protected readonly section = computed(() => {
    const ids = this.sections().map((s) => s.id);
    const picked = this.picked();
    return picked && ids.includes(picked) ? picked : (ids[0] ?? null);
  });
  protected readonly current = computed(() => this.sections().find((s) => s.id === this.section()));
  protected readonly listOpen = signal(true);

  constructor() {
    // Nothing left to set (left, removed, lost the permissions): close.
    effect(() => {
      if (!this.community() || !this.sections().length) this.close();
    });
  }

  protected open(section: Section): void {
    this.picked.set(section);
    this.listOpen.set(false);
  }

  protected close(): void {
    this.shell.serverSettings.set(false);
  }

  /* --- OVERVIEW --- */
  protected readonly overview = signal<State>('idle');
  protected readonly image = signal<'idle' | 'picture' | 'banner' | CmdError['code']>('idle');

  protected async saveOverview(name: string, about: string): Promise<void> {
    if (!name.trim()) return;
    this.overview.set('saving');
    const ok = await this.store.update({ name: name.trim(), about: about.trim() });
    this.overview.set(ok ? 'done' : (this.store.error() ?? 'internal'));
  }

  protected async setImage(input: HTMLInputElement, banner: boolean): Promise<void> {
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    this.image.set(banner ? 'banner' : 'picture');
    const ok = await this.store.setPicture(file, banner);
    this.image.set(ok ? 'idle' : (this.store.error() ?? 'internal'));
  }

  /* --- ROLES: edited as a draft, saved as one list --- */
  /** The roles being edited; reset whenever the community's change. */
  protected readonly draft = linkedSignal(() =>
    (this.community()?.all_roles ?? []).map((r) => ({ ...r, attrs: [...r.attrs] })),
  );
  protected readonly picking = linkedSignal<Role[], string | null>({
    source: this.draft,
    computation: (roles, previous) =>
      previous?.value && roles.some((r) => r.id === previous.value)
        ? previous.value
        : (roles[0]?.id ?? null),
  });
  protected readonly role = computed(() => this.draft().find((r) => r.id === this.picking()));
  protected readonly dirty = computed(
    () => JSON.stringify(this.draft()) !== JSON.stringify(this.community()?.all_roles ?? []),
  );
  /** The relay refuses a role without a name (spec § 9). */
  protected readonly named = computed(() => this.draft().every((r) => r.name.trim()));
  protected readonly rolesState = signal<State>('idle');

  /** Below my own, or new: mine to edit (the relay checks the same, spec § 9). */
  protected manageable(role: Role): boolean {
    const c = this.community();
    return (
      !!c && (c.manageable_roles.includes(role.id) || !c.all_roles.some((r) => r.id === role.id))
    );
  }

  /** A role may swap with its neighbour if both are mine and neither is `everyone` (always last). */
  protected canMove(index: number, by: -1 | 1): boolean {
    const roles = this.draft();
    const [a, b] = [roles[index], roles[index + by]];
    return (
      !!a &&
      !!b &&
      a.id !== EVERYONE &&
      b.id !== EVERYONE &&
      this.manageable(a) &&
      this.manageable(b)
    );
  }

  protected move(index: number, by: -1 | 1): void {
    if (!this.canMove(index, by)) return;
    this.draft.update((roles) => {
      const next = [...roles];
      [next[index], next[index + by]] = [next[index + by], next[index]];
      return next;
    });
  }

  protected addRole(): void {
    const role: Role = {
      id: roleId(),
      name: $localize`:@@role.new:New role`,
      color: '',
      attrs: [],
    };
    // Just above everyone: the lowest, like a new member.
    this.draft.update((roles) => [...roles.slice(0, -1), role, ...roles.slice(-1)]);
    this.picking.set(role.id);
  }

  protected deleteRole(role: Role): void {
    this.draft.update((roles) => roles.filter((r) => r.id !== role.id));
  }

  protected editRole(change: Partial<Role>): void {
    const id = this.picking();
    this.draft.update((roles) => roles.map((r) => (r.id === id ? { ...r, ...change } : r)));
  }

  protected toggle(attr: string): void {
    const role = this.role();
    if (!role) return;
    const attrs = role.attrs.includes(attr)
      ? role.attrs.filter((a) => a !== attr)
      : [...role.attrs, attr];
    this.editRole({ attrs });
  }

  /** I can only give what I have; taking away is always fine. */
  protected canGrant(role: Role, perm: Permission): boolean {
    return this.manageable(role) && (role.attrs.includes(perm) || this.store.can(perm));
  }

  protected async saveRoles(): Promise<void> {
    this.rolesState.set('saving');
    const ok = await this.store.update({ roles: this.draft() });
    this.rolesState.set(ok ? 'done' : (this.store.error() ?? 'internal'));
  }

  protected resetRoles(): void {
    this.draft.set((this.community()?.all_roles ?? []).map((r) => ({ ...r, attrs: [...r.attrs] })));
    this.rolesState.set('idle');
  }

  /* --- MEMBERS --- */
  protected readonly me = computed(() => this.account.user()?.pubkey);
  /** Roles I may give or take (not everyone: everybody has it). */
  protected readonly givable = computed(() =>
    (this.community()?.all_roles ?? []).filter(
      (r) => r.id !== EVERYONE && this.community()?.manageable_roles.includes(r.id),
    ),
  );

  protected card(pubkey: string): UserShort {
    return {
      id: pubkey,
      username: this.dm.label(pubkey),
      picture: this.dm.peers()[pubkey]?.picture,
    };
  }

  protected roleName(id: string): string {
    return this.community()?.all_roles.find((r) => r.id === id)?.name ?? id;
  }

  protected roleColor(id: string): string | null {
    return this.community()?.all_roles.find((r) => r.id === id)?.color || null;
  }

  protected toggleRole(member: Member, role: string): void {
    const roles = member.roles.includes(role)
      ? member.roles.filter((r) => r !== role)
      : [...member.roles, role];
    void this.store.setRoles(member.pubkey, roles);
  }

  protected removeMember(member: Member): void {
    void this.store.removeMember(member.pubkey);
  }

  /* --- OWNERSHIP --- */
  protected readonly heirs = computed(() => this.store.members().filter((m) => !m.owner));
  protected readonly heir = signal('');
  protected readonly confirmTransfer = signal(false);
  protected readonly confirmDelete = signal(false);
  protected readonly ownership = signal<State>('idle');

  protected async transfer(): Promise<void> {
    this.ownership.set('saving');
    const ok = await this.store.update({ owner: this.heir() });
    this.confirmTransfer.set(false);
    this.ownership.set(ok ? 'done' : (this.store.error() ?? 'internal'));
  }

  protected async deleteCommunity(): Promise<void> {
    this.ownership.set('saving');
    if (await this.store.deleteCommunity()) {
      this.close();
    } else {
      this.ownership.set(this.store.error() ?? 'internal');
    }
  }
}
