<!-- exp=baseline-001 task=macro_playlist_player cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.05247999999999999 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Ocean Eyes", "artist": "Billie Eilish", "duration": "3:20"},
            {"title": "Sunflower", "artist": "Post Malone", "duration": "2:38"},
            {"title": "Circles", "artist": "Post Malone", "duration": "3:35"},
            {"title": "Falling", "artist": "Trevor Daniel", "duration": "2:39"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Weightless", "artist": "Marconi Union", "duration": "8:08"},
            {"title": "Divenire", "artist": "Ludovico Einaudi", "duration": "6:42"},
            {"title": "Time", "artist": "Hans Zimmer", "duration": "4:35"},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Pump",
        "tracks": [
            {"title": "Stronger", "artist": "Kanye West", "duration": "5:11"},
            {"title": "Till I Collapse", "artist": "Eminem", "duration": "4:57"},
            {"title": "Power", "artist": "Kanye West", "duration": "4:52"},
            {"title": "Can't Hold Us", "artist": "Macklemore", "duration": "4:18"},
            {"title": "Believer", "artist": "Imagine Dragons", "duration": "3:24"},
        ],
    },
]


def find_playlist(playlist_id):
    for pl in PLAYLISTS:
        if pl["id"] == playlist_id:
            return pl
    return PLAYLISTS[0]


@component
def PlaylistPlayer():
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    def track_key(track):
        return f"{track['title']}-{track['artist']}"

    now_key = track_key(now_playing) if now_playing is not None else None

    return div(class_name="playlist-player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2(class_name="sidebar-title", "Playlists"),
                ul(class_name="playlist-list",
                    *[li(key=pl["id"],
                         class_name="playlist-item active" if pl["id"] == active_id else "playlist-item",
                         on_click=lambda pid=pl["id"]: select_playlist(pid),
                         span(class_name="playlist-name", pl["name"]),
                         span(class_name="playlist-count", f"{len(pl['tracks'])} tracks"))
                      for pl in PLAYLISTS]),
            ),
            main(class_name="main-panel",
                h1(class_name="panel-title", active["name"]),
                table(class_name="track-table",
                    thead(
                        tr(
                            th(class_name="col-num", "#"),
                            th(class_name="col-title", "Title"),
                            th(class_name="col-artist", "Artist"),
                            th(class_name="col-duration", "Duration"),
                        ),
                    ),
                    tbody(
                        *[tr(key=track_key(track),
                             class_name="track-row playing" if track_key(track) == now_key else "track-row",
                             on_click=lambda t=track: play_track(t),
                             td(class_name="col-num", str(i)),
                             td(class_name="col-title", track["title"]),
                             td(class_name="col-artist", track["artist"]),
                             td(class_name="col-duration", track["duration"]))
                          for i, track in enumerate(active["tracks"], start=1)]),
                ),
            ),
        ),
        footer(class_name="now-playing-bar",
            div(class_name="now-playing-info",
                span(class_name="now-playing-label", "Now Playing:"),
                span(class_name="now-playing-title",
                     f"{now_playing['title']} — {now_playing['artist']}" if now_playing is not None else "Nothing playing"),
            ),
            button(class_name="play-toggle",
                   on_click=lambda: toggle_play(),
                   disabled=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
