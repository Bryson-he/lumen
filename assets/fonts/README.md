# Bundled fonts

Used by the desktop widgets (`lumen-render` loads them into DirectWrite, so they
do not need to be installed in Windows). The settings app bundles the same
families through the `@fontsource` npm packages.

- Inter — SIL Open Font License 1.1 (`LICENSE-Inter.txt`)
- Space Grotesk — SIL Open Font License 1.1 (`LICENSE-SpaceGrotesk.txt`)

Files are the Latin WOFF2 subsets from `@fontsource/inter` and
`@fontsource/space-grotesk`. Other scripts fall back to Windows' own fonts.
