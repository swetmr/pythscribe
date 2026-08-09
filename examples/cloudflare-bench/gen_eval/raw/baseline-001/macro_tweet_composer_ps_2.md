<!-- exp=baseline-001 task=macro_tweet_composer cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.032766 -->
```python
"use client"

from pyths.react import component, use_state

MAX_LEN = 280

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just proved a theorem in my head on the tram. Beautiful.", "likes": 42},
    {"id": 2, "author": "@grace", "text": "Found a bug. It was, quite literally, a moth.", "likes": 128},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 91},
]


def remaining_class(remaining):
    if remaining < 0:
        return "counter over"
    if remaining <= 20:
        return "counter warn"
    return "counter"


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    text, set_text = use_state("")
    next_id, set_next_id = use_state(len(SEED_TWEETS) + 1)

    remaining = MAX_LEN - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": next_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_next_id(next_id + 1)
        set_text("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    return div(class_name="tweet-feed",
        div(class_name="composer",
            textarea(
                class_name="composer-input",
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="What's happening?",
                rows=3,
            ),
            div(class_name="composer-footer",
                span(class_name=remaining_class(remaining), f"{remaining}"),
                button(
                    class_name="post-button",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Tweet",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                div(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}",
                ),
              ) for t in tweets]
        ),
    )
```
