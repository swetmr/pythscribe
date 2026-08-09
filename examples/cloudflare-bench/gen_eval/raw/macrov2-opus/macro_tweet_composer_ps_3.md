<!-- exp=macrov2-opus task=macro_tweet_composer cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.031628 -->
```python
"use client"

from pyths.react import component, use_state

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "The Analytical Engine weaves algebraic patterns.", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Found an actual bug in the relay today.", "likes": 8},
    {"id": 3, "author": "@alan", "text": "Can machines think? Asking for a friend.", "likes": 42},
]

MAX_CHARS = 280


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    text, set_text = use_state("")

    remaining = MAX_CHARS - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": (tweets[0]["id"] + 1) if tweets else 1,
            "author": "@you",
            "text": text.strip(),
            "likes": 0,
        }
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    counter_class = "char-counter over" if remaining < 0 else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="compose",
            textarea(
                class_name="compose-input",
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="What's happening?",
            ),
            div(class_name="compose-footer",
                span(class_name=counter_class, str(remaining)),
                button(
                    class_name="post-btn",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                span(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-btn",
                    on_click=lambda: like(t["id"]),
                    f"♥ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```
