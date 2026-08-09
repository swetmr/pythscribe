<!-- exp=baseline-001 task=macro_playlist_player cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.04932999999999999 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Sunset Drive", "artist": "Nova Waves", "duration": "3:42"},
            {"title": "Ocean Breeze", "artist": "Kai Lumen", "duration": "4:15"},
            {"title": "Slow Motion", "artist": "Mira Sol", "duration": "3:08"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Neon Circuit", "artist": "Byte Forest", "duration": "5:20"},
            {"title": "Quiet Hours", "artist": "Ana Petrov", "duration": "4:47"},
            {"title": "Paper Trails", "artist": "The Loop", "duration": "3:55"},
            {"title": "Blue Static", "artist": "Halcyon", "duration": "6:02"},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Pump",
        "tracks": [
            {"title": "Redline", "artist": "Volt Kings", "duration": "2:58"},
            {"title": "Ignite", "artist": "Ember Squad", "duration": "3:31"},
            {"title": "No Brakes", "artist": "Turbo Lane", "duration": "3:12"},
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

    def is_current(track):
        return now_playing is not None and now_playing["title"] == track["title"] \
            and now_playing["artist"] == track["artist"]

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
            section(class_name="track-panel",
                h1(class_name="panel-title", active["name"]),
                div(class_name="track-header",
                    span(class_name="col-title", "Title"),
                    span(class_name="col-artist", "Artist"),
                    span(class_name="col-duration", "Duration")),
                ul(class_name="track-list",
                    *[li(key=f"{active['id']}-{i}",
                         class_name="track-item playing" if is_current(track) else "track-item",
                         on_click=lambda t=track: play_track(t),
                         span(class_name="col-title", track["title"]),
                         span(class_name="col-artist", track["artist"]),
                         span(class_name="col-duration", track["duration"]))
                      for i, track in enumerate(active["tracks"])]),
            ),
        ),
        footer(class_name="now-playing-bar",
            div(class_name="now-playing-info",
                now_playing is not None
                    and span(class_name="np-title", f"{now_playing['title']} — {now_playing['artist']}")
                    or span(class_name="np-title muted", "Nothing playing")),
            button(class_name="play-toggle",
                   disabled=now_playing is None,
                   on_click=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
