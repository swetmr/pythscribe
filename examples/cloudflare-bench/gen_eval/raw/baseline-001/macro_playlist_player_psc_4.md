<!-- exp=baseline-001 task=macro_playlist_player cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.041769999999999995 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Klyn", "duration": "3:42"},
        {"title": "Ocean Eyes", "artist": "Marlo", "duration": "4:05"},
        {"title": "Slow Down", "artist": "Neive", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Focus Flow", "tracks": [
        {"title": "Deep Work", "artist": "Cortex", "duration": "5:12"},
        {"title": "Rain Loop", "artist": "Ambia", "duration": "6:30"},
    ]},
    {"id": 3, "name": "Morning Run", "tracks": [
        {"title": "Uphill", "artist": "Pace", "duration": "3:20"},
        {"title": "Second Wind", "artist": "Strider", "duration": "3:55"},
        {"title": "Finish Line", "artist": "Tempo", "duration": "4:10"},
        {"title": "Cooldown", "artist": "Ease", "duration": "2:41"},
    ]},
]


def find_playlist(pid):
    for p in PLAYLISTS:
        if p["id"] == pid:
            return p
    return PLAYLISTS[0]


@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(cn="player",
        div(cn="sidebar",
            h2(cn="sidebar-title", "Playlists"),
            ul(cn="playlist-list",
                *[li(key=p["id"],
                     cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                     oc=lambda p=p: select_playlist(p["id"]),
                     span(cn="playlist-name", p["name"]),
                     span(cn="playlist-count", f"{len(p['tracks'])} tracks"))
                  for p in PLAYLISTS]),
        ),
        div(cn="main",
            h1(cn="main-title", active["name"]),
            ul(cn="track-list",
                *[li(key=i,
                     cn="track-item playing" if now_playing and now_playing["title"] == t["title"] else "track-item",
                     oc=lambda t=t: play_track(t),
                     span(cn="track-title", t["title"]),
                     span(cn="track-artist", t["artist"]),
                     span(cn="track-duration", t["duration"]))
                  for i, t in enumerate(active["tracks"])]),
        ),
        div(cn="now-bar",
            span(cn="now-label",
                 f"Now Playing: {now_playing['title']} — {now_playing['artist']}" if now_playing else "Nothing playing"),
            button(cn="play-toggle",
                   dis=now_playing is None,
                   oc=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
