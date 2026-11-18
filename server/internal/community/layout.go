package community

import (
	"errors"
	"fmt"
	"regexp"
	"slices"
	"strconv"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/nip29"
)

// Limits of spec § 9, Salons et catégories.
const (
	maxChannels   = 500
	maxCategories = 50
	maxSlowmode   = 21600
	maxUserLimit  = 99
	maxName       = 100
	maxTopic      = 1024
)

var categoryRe = regexp.MustCompile(`^[a-z0-9_-]{1,32}$`)

// Category groups channels of a community.
type Category struct{ ID, Name string }

// layout is what the library drops from a 39000. A community: its categories
// and the one each channel sits in. A channel: slow mode and user limit.
type layout struct {
	Categories []Category
	In         map[string]string // channel → category id
	Slowmode   int
	UserLimit  int
}

func parseLayout(tags nostr.Tags) *layout {
	l := &layout{In: map[string]string{}}
	for _, t := range tags {
		switch {
		case len(t) >= 3 && t[0] == "category":
			l.Categories = append(l.Categories, Category{t[1], t[2]})
		case len(t) >= 3 && t[0] == "child" && t[2] != "":
			l.In[t[1]] = t[2]
		case len(t) >= 2 && t[0] == "slowmode":
			l.Slowmode, _ = strconv.Atoi(t[1])
		case len(t) >= 2 && t[0] == "user_limit":
			l.UserLimit, _ = strconv.Atoi(t[1])
		}
	}
	return l
}

func (l *layout) has(category string) bool {
	return slices.ContainsFunc(l.Categories, func(c Category) bool { return c.ID == category })
}

// placed: where each of children sits, only in categories that exist.
func (l *layout) placed(children []string) map[string]string {
	out := map[string]string{}
	for _, ch := range children {
		if cat := l.In[ch]; cat != "" && l.has(cat) {
			out[ch] = cat
		}
	}
	return out
}

// tags of a community's layout: categories, and child tags with their category.
func (l *layout) apply(meta *nostr.Event) {
	for i, t := range meta.Tags {
		if len(t) == 2 && t[0] == "child" && l.In[t[1]] != "" {
			meta.Tags[i] = nostr.Tag{"child", t[1], l.In[t[1]]}
		}
	}
	for _, c := range l.Categories {
		meta.Tags = append(meta.Tags, nostr.Tag{"category", c.ID, c.Name})
	}
	if l.Slowmode > 0 {
		meta.Tags = append(meta.Tags, nostr.Tag{"slowmode", strconv.Itoa(l.Slowmode)})
	}
	if l.UserLimit > 0 {
		meta.Tags = append(meta.Tags, nostr.Tag{"user_limit", strconv.Itoa(l.UserLimit)})
	}
}

// checkCategories: a community 9002's categories, and child tags naming only those.
func checkCategories(tags nostr.Tags) error {
	seen := map[string]bool{}
	for t := range tags.FindAll("category") {
		switch {
		case len(t) < 3 || !categoryRe.MatchString(t[1]):
			return errors.New("invalid: a category id is 1 to 32 characters a-z 0-9 _ -")
		case seen[t[1]]:
			return fmt.Errorf("invalid: category %q twice", t[1])
		case t[2] == "" || len(t[2]) > maxName:
			return errors.New("invalid: a category's name is 1 to 100 bytes")
		}
		seen[t[1]] = true
	}
	if len(seen) > maxCategories {
		return fmt.Errorf("invalid: %d categories at most", maxCategories)
	}
	for t := range tags.FindAll("child") {
		if len(t) >= 3 && t[2] != "" && !seen[t[2]] {
			return fmt.Errorf("invalid: unknown category %q", t[2])
		}
	}
	return nil
}

// checkChannel: a channel's 9007 (group nil) or 9002.
func checkChannel(ev nostr.Event, group *nip29.Group) error {
	if name := tagValue(ev, "name"); name == "" || len(name) > maxName {
		return errors.New("invalid: a channel's name is 1 to 100 bytes")
	}
	if len(tagValue(ev, "about")) > maxTopic {
		return errors.New("invalid: a topic is 1024 bytes at most")
	}
	if group != nil && ev.Tags.Has("livekit") != group.LiveKit {
		return errors.New("invalid: a channel's type is set when it's created")
	}
	if t := ev.Tags.Find("slowmode"); t != nil {
		if n, err := strconv.Atoi(t[1]); err != nil || n < 0 || n > maxSlowmode {
			return fmt.Errorf("invalid: slow mode is 0 to %d seconds", maxSlowmode)
		}
	}
	if t := ev.Tags.Find("user_limit"); t != nil {
		if n, err := strconv.Atoi(t[1]); err != nil || n < 1 || n > maxUserLimit {
			return fmt.Errorf("invalid: a user limit is 1 to %d", maxUserLimit)
		}
	}
	return nil
}

// arrange: the channels of given in its order (unknown ones and repeats
// dropped), then those of current it missed. Channels come and go by 9007
// and 9008 only: a client that hasn't seen a new one yet doesn't erase it.
func arrange(current, given []string) []string {
	out := make([]string, 0, len(current))
	for _, ch := range given {
		if slices.Contains(current, ch) && !slices.Contains(out, ch) {
			out = append(out, ch)
		}
	}
	for _, ch := range current {
		if !slices.Contains(out, ch) {
			out = append(out, ch)
		}
	}
	return out
}

func childIDs(tags nostr.Tags) []string {
	var ids []string
	for t := range tags.FindAll("child") {
		ids = append(ids, t[1])
	}
	return ids
}
