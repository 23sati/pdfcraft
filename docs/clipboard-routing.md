# Page clipboard routing

The page organizer and Pages sidebar dispatch clipboard gestures through `page.copy`,
`page.cut` and `page.paste`. Those commands apply the same permission checks as menus.
Focused text and form controls retain their clipboard events. The Organize toolbar no
longer consumes the same event independently.

The regression test covers raw-key plus logical clipboard events arriving on separate
frames. Browser menu events without a matching key remain independent commands.
Real browser permission handling and delayed browser events still need manual review.
This contribution contains no session persistence, launcher or branding changes.
