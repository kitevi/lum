# Radio Playback Drops Pause and Resume

Status: Accepted for the command line. ADR-0022 brought pause back as the tray's Stop/Play toggle, so playback is no longer only stopped or playing.

## Context

`lum radio` used to model playback as three states: stopped, playing, and paused. Because playback is delegated to a detached `ffplay` process, pause was implemented by stopping `ffplay` and recording the station as paused. Resume started a new `ffplay` process for that remembered station.

That interface added a paused flag to `radio-player.json`, two CLI commands, two dispatch paths, and status output for a state that behaved like stop-then-play.

## Decision

Remove pause and resume from the `lum radio` command line: there are no `pause` and `resume` commands, and the stored radio state does not hold a paused flag.

## Consequences

From the command line, playback is stopped or playing. `status` prints `playing <code> <description>` only when the remembered process is still alive. Otherwise it clears stale state and prints `stopped`.

`pause` and `resume` are no longer reserved radio commands. They are parsed like any other station code and will fail as unknown stations unless a station with that code exists.

ADR-0022 later brought pause back without widening the command line: the tray's Stop/Play toggle pauses, the runner records `paused` in `radio-player.json`, and `status` prints `paused <code> <description>` while it is paused. The tray's pause is a toggle on a station that stays loaded, not a command to learn.

If future requirements need pause-like behavior on the command line, prefer an explicit new ADR before widening the radio interface again.
