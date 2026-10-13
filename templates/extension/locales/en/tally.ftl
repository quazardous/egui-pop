# The tally extension's own text, in English. Every message carries the
# plugin's id (`tally-…`, `command-tally-…`, `plugin-tally`), so it can
# never rewrite another plugin's text.

plugin-tally =
    .title = Tally

tally-decrease = Decrease { $name }
tally-increase = Increase { $name }
tally-value = { $name }: { $count }
tally-missing = The tally plugin is not connected.

command-tally-increment =
    .title = Increase a tally
    .description = Adds one to a tally (`tally`: its name).
command-tally-set =
    .title = Set a tally
    .description = Sets a tally to a value (`tally`: its name, `value`: a whole number).
