# Test fonts

- `InterVariable-subset.ttf`: [Inter](https://github.com/rsms/inter)
  (Inter Variable, SIL Open Font License 1.1, `Inter-OFL.txt`), subset to
  the letters of "Hamburgefonstiv", their capitals and the digits, with
  its `wght` axis (100 to 900) kept and `opsz` dropped, for the tests of
  weights drawn through a variation axis. Made with fontTools
  (`pyftsubset`, then `varLib.instancer opsz=drop`).
