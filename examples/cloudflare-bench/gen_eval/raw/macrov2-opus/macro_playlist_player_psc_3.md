<!-- exp=macrov2-opus task=macro_playlist_player cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.043653 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "chill", "name": "Chill Vibes", "tracks": [
        {"title": "Ocean Drive", "artist": "Duke Dumont", "duration": "3:47"},
        {"title": "Sunflower", "artist": "Post Malone", "duration": "2:38"},
        {"title": "Blinding Lights", "artist": "The Weeknd", "duration": "3:20"},
    ]},
    {"id": "focus", "name": "Deep Focus", "tracks": [
        {"title": "Weightless", "artist": "Marconi Union", "duration": "8:10"},
        {"title": "Nuvole Bianche", "artist": "Ludovico Einaudi", "duration": "5:57"},
    ]},
    {"id": "party", "name": "Party Starters", "tracks": [
        {"title": "One More Time", "artist": "Daft Punk", "duration": "5:20"},
        {"title": "Levels", "artist": "Avicii", "duration": "3:19"},
        {"title": "Titanium", "artist": "David Guetta", "duration": "4:05"},
        {"title": "Uptown Funk", "artist": "Mark Ronson", "duration": "4:30"},
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
        set_now_playing(None)
        set_is_playing(False)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="player-body",
            aside(cn="sidebar",
                h2(cn="sidebar-title", "Playlists"),
                ul(cn="playlist-list",
                    *[li(key=p["id"],
                        cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                        oc=lambda: select_playlist(p["id"]),
                        span(cn="playlist-name", p["name"]),
                        span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in PLAYLISTS],
                ),
            ),
            main(cn="main-panel",
                h1(cn="panel-title", active["name"]),
                ul(cn="track-list",
                    *[li(key=i,
                        cn="track-row now-playing" if now_playing is not None and now_playing["title"] == t["title"] else "track-row",
                        oc=lambda: play_track(t),
                        span(cn="track-title", t["title"]),
                        span(cn="track-artist", t["artist"]),
                        span(cn="track-duration", t["duration"]),
                    ) for i, t in enumerate(active["tracks"])],
                ),
            ),
        ),
        div(cn="now-bar",
            div(cn="now-info",
                span(cn="now-label", "Now Playing:"),
                span(cn="now-title",
                    now_playing["title"] if now_playing is not None else "—"),
            ),
            button(cn="play-toggle",
                dis=now_playing is None,
                oc=lambda: toggle_play(),
                "Pause" if is_playing else "Play"),
        ),
    )
```
