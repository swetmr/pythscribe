<!-- exp=baseline-001 task=macro_playlist_player cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.052904999999999994 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Weightless", "artist": "Marconi Union", "duration": "8:08"},
            {"title": "An Ending", "artist": "Brian Eno", "duration": "4:22"},
            {"title": "Nuvole Bianche", "artist": "Ludovico Einaudi", "duration": "5:57"},
            {"title": "Saturn", "artist": "Sleeping At Last", "duration": "4:50"},
        ],
    },
    {
        "id": "throwback",
        "name": "90s Throwback",
        "tracks": [
            {"title": "Wonderwall", "artist": "Oasis", "duration": "4:18"},
            {"title": "No Diggity", "artist": "Blackstreet", "duration": "5:04"},
            {"title": "Bittersweet Symphony", "artist": "The Verve", "duration": "5:58"},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Energy",
        "tracks": [
            {"title": "Till I Collapse", "artist": "Eminem", "duration": "4:57"},
            {"title": "Stronger", "artist": "Kanye West", "duration": "5:11"},
            {"title": "Can't Hold Us", "artist": "Macklemore", "duration": "4:18"},
            {"title": "POWER", "artist": "Kanye West", "duration": "4:52"},
            {"title": "Titanium", "artist": "David Guetta", "duration": "4:05"},
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

    def select_playlist(pl_id):
        set_active_id(pl_id)
        set_now_playing(None)
        set_is_playing(False)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    def is_current(track):
        return now_playing is not None and now_playing["title"] == track["title"] and now_playing["artist"] == track["artist"]

    return div(class_name="playlist-player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2(class_name="sidebar-title", "Your Library"),
                ul(class_name="playlist-list",
                    *[li(key=pl["id"],
                        class_name="playlist-item active" if pl["id"] == active_id else "playlist-item",
                        on_click=lambda pl_id=pl["id"]: select_playlist(pl_id),
                        span(class_name="playlist-name", pl["name"]),
                        span(class_name="playlist-count", f"{len(pl['tracks'])} tracks"),
                    ) for pl in PLAYLISTS],
                ),
            ),
            main(class_name="track-panel",
                h1(class_name="panel-title", active["name"]),
                p(class_name="panel-subtitle", f"{len(active['tracks'])} songs"),
                ul(class_name="track-list",
                    *[li(key=f"{active_id}-{i}",
                        class_name="track-row playing" if is_current(track) else "track-row",
                        on_click=lambda t=track: play_track(t),
                        span(class_name="track-index", f"{i + 1}"),
                        div(class_name="track-meta",
                            span(class_name="track-title", track["title"]),
                            span(class_name="track-artist", track["artist"]),
                        ),
                        span(class_name="track-duration", track["duration"]),
                    ) for i, track in enumerate(active["tracks"])],
                ),
            ),
        ),
        footer(class_name="now-playing-bar",
            div(class_name="now-playing-info",
                span(class_name="now-playing-label", "Now Playing"),
                span(class_name="now-playing-title",
                     now_playing["title"] if now_playing is not None else "Nothing playing"),
                span(class_name="now-playing-artist",
                     now_playing["artist"] if now_playing is not None else "Select a track"),
            ),
            button(class_name="play-toggle",
                   on_click=lambda: toggle_play(),
                   disabled=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
