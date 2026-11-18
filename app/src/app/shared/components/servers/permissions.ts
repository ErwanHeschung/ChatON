import { Permission } from '@models/Community.model';

/** The permissions a role gives, grouped as the editor shows them (spec § 9). */
export const PERMISSION_GROUPS: {
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
