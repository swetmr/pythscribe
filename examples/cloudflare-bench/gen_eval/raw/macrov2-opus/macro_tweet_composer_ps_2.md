<!-- exp=macrov2-opus task=macro_tweet_composer cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.031553000000000005 -->
```python
"use client"

from pyths.react import component, use_state

MAX_LEN = 280

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just wrote my first PythScribe component!", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Debugging is twice as hard as writing the code.", "likes": 34},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 128},
]


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    text, set_text = use_state("")

    remaining = MAX_LEN - len(text)
    over_limit = remaining < 0
    can_post = len(text.strip()) > 0 and not over_limit

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": tweets[0]["id"] + 1 if tweets else 1,
            "author": "@me",
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

    counter_class = "char-counter over-limit" if over_limit else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="composer",
            textarea(
                class_name="composer-input",
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="What's happening?",
            ),
            div(class_name="composer-footer",
                span(class_name=counter_class, str(remaining)),
                button(
                    class_name="post-button",
                    on_click=lambda: post(),
                    disabled=not can_post,
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                div(class_name="tweet-author", t["author"]),
                div(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda: like(t["id"]),
                    f"♥ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```
