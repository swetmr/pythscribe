<!-- exp=baseline-001 task=macro_playlist_player cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.054604999999999994 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "pl_focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Weightless", "artist": "Marconi Union", "duration": "8:08"},
            {"title": "An Ending (Ascent)", "artist": "Brian Eno", "duration": "4:24"},
            {"title": "Divenire", "artist": "Ludovico Einaudi", "duration": "6:42"},
            {"title": "Saturn", "artist": "Sleeping At Last", "duration": "4:50"},
        ],
    },
    {
        "id": "pl_workout",
        "name": "Cardio Rush",
        "tracks": [
            {"title": "Titanium", "artist": "David Guetta", "duration": "4:05"},
            {"title": "Stronger", "artist": "Kanye West", "duration": "5:11"},
            {"title": "Can't Hold Us", "artist": "Macklemore", "duration": "4:18"},
        ],
    },
    {
        "id": "pl_chill",
        "name": "Sunday Chill",
        "tracks": [
            {"title": "Sunflower", "artist": "Post Malone", "duration": "2:38"},
            {"title": "Coffee", "artist": "Beabadoobee", "duration": "3:09"},
            {"title": "Electric Feel", "artist": "MGMT", "duration": "3:49"},
            {"title": "Redbone", "artist": "Childish Gambino", "duration": "5:26"},
            {"title": "The Less I Know", "artist": "Tame Impala", "duration": "3:36"},
        ],
    },
]


def find_playlist(playlist_id):
    for pl in PLAYLISTS:
        if pl["id"] == playlist_id:
            return pl
    return PLAYLISTS[0]


def track_key(playlist_id, index):
    return f"{playlist_id}:{index}"


@component
def PlaylistPlayer():
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing_key, set_now_playing_key = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active = find_playlist(active_id)

    def select_playlist(pl_id):
        set_active_id(pl_id)

    def play_track(index):
        set_now_playing_key(track_key(active_id, index))
        set_is_playing(True)

    def toggle_play():
        if now_playing_key is not None:
            set_is_playing(not is_playing)

    now_track = None
    for i, t in enumerate(active["tracks"]):
        if track_key(active_id, i) == now_playing_key:
            now_track = t

    return div(class_name="playlist-player",
        div(class_name="pp-body",
            aside(class_name="pp-sidebar",
                h2(class_name="pp-sidebar-title", "Your Library"),
                ul(class_name="pp-playlist-list",
                    *[li(key=pl["id"],
                         class_name="pp-playlist-item active" if pl["id"] == active_id
                                    else "pp-playlist-item",
                         on_click=lambda pl_id=pl["id"]: select_playlist(pl_id),
                         span(class_name="pp-playlist-name", pl["name"]),
                         span(class_name="pp-track-count", f"{len(pl['tracks'])} tracks"))
                      for pl in PLAYLISTS]),
            ),
            section(class_name="pp-main",
                h1(class_name="pp-main-title", active["name"]),
                ul(class_name="pp-track-list",
                    *[li(key=track_key(active_id, i),
                         class_name="pp-track playing"
                                    if track_key(active_id, i) == now_playing_key
                                    else "pp-track",
                         on_click=lambda idx=i: play_track(idx),
                         span(class_name="pp-track-number", f"{i + 1}"),
                         div(class_name="pp-track-meta",
                             span(class_name="pp-track-title", t["title"]),
                             span(class_name="pp-track-artist", t["artist"])),
                         span(class_name="pp-track-duration", t["duration"]))
                      for i, t in enumerate(active["tracks"])]),
            ),
        ),
        footer(class_name="pp-now-bar",
            div(class_name="pp-now-info",
                span(class_name="pp-now-label", "Now Playing"),
                span(class_name="pp-now-title",
                     now_track["title"] if now_track is not None else "Nothing playing"),
                span(class_name="pp-now-artist",
                     now_track["artist"] if now_track is not None else "—")),
            button(class_name="pp-play-toggle",
                   disabled=now_playing_key is None,
                   on_click=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
