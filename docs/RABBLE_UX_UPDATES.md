# Rabble UX Updates Needed

## Design System Alignment with ABW

### Typography
- Use Gruvbox color palette (match Agent Bestiary)
- Font family: system-ui or similar to ABW
- All instances of "Rabble.world" → "rabble.world" (lowercase)

### Color Palette (from variables.css)
```css
--bg0-hard: #1d2021;
--bg0: #282828;
--bg1: #3c3836;
--bg2: #504945;
--bg3: #665c54;
--fg0: #fbf1c7;
--fg1: #ebdbb2;
--fg2: #d5c4a1;
--fg3: #bdae93;
--yellow: #fabd2f;
--green: #b8bb26;
--red: #fb4934;
--blue: #83a598;
--purple: #d3869b;
--aqua: #8ec07c;
--orange: #fe8019;
--gray: #928374;
```

### Button Styling
- **Remove**: Chunky green buttons
- **Replace with**: Tufte-style minimal design
  - Lighter weight borders (1-2px max)
  - More whitespace/breathing room
  - Subtle hover states
  - Use Gruvbox `--aqua: #8ec07c` or `--fg2: #d5c4a1` for borders
  - Transparent or minimal backgrounds

### Icons
- ✅ **Butterfly icon**: Created at `static/creatures/butterfly.svg`
  - Monarch butterfly style (orange `#E8A838`)
  - Outlined stroke design
  - Minimal, elegant
  
- ✅ **Dragonfly icon**: Created at `static/creatures/dragonfly.svg`
  - Blue lines (`#5A9BD5`)
  - Same outlined stroke style
  - Elongated body

### Flutter Source Location
The Flutter source code is NOT in this repository. The compiled web app is at:
- `static/rabble/` (compiled output)

To make these changes:
1. Find the Flutter source repository
2. Update theme in `lib/theme.dart` or similar
3. Update all "Rabble.world" → "rabble.world" in UI strings
4. Update button widgets to use minimal Tufte-style
5. Update creature icons to use new SVGs
6. Rebuild with `flutter build web`
7. Copy output to `static/rabble/`

## Implementation Checklist
- [x] Create butterfly.svg icon
- [x] Create dragonfly.svg icon
- [ ] Update Flutter theme to use Gruvbox colors
- [ ] Replace button styling with minimal Tufte aesthetic
- [ ] Update all "Rabble.world" text to lowercase
- [ ] Increase whitespace in layouts
- [ ] Use lighter font weights
- [ ] Test on mobile and desktop
- [ ] Rebuild and deploy
