<!-- exp=baseline-001 task=macro_tweet_composer cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.032891000000000004 -->
```python
"use client"

from pyths.react import component, use_state

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just shipped the compiler backend. Feels good.", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Debugging is twice as hard as writing the code in the first place.", "likes": 47},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 128},
]

MAX_LEN = 280


def next_id(tweets):
    return max([t["id"] for t in tweets]) + 1 if tweets else 1


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    text, set_text = use_state("")

    remaining = MAX_LEN - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": next_id(tweets), "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    return div(class_name="tweet-feed",
        h2(class_name="tweet-feed-title", "Home"),
        div(class_name="tweet-compose",
            textarea(
                class_name="tweet-compose-input",
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="What's happening?",
                rows=3,
            ),
            div(class_name="tweet-compose-actions",
                span(
                    class_name="tweet-char-count over" if remaining < 0 else "tweet-char-count",
                    f"{remaining}",
                ),
                button(
                    class_name="tweet-post-button",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Tweet",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet-item",
                span(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                button(
                    class_name="tweet-like-button",
                    on_click=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```
