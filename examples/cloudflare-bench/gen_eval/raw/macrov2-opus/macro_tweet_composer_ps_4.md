<!-- exp=macrov2-opus task=macro_tweet_composer cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.031453 -->
```python
"use client"

from pyths.react import component, use_state

SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just shipped a new compiler pass. It floors correctly now.", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Debugging is twice as hard as writing the code. So write it clearly.", "likes": 47},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 128},
]

MAX_LEN = 280


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
            ),
            div(class_name="composer-footer",
                span(class_name=f"char-counter {'over-limit' if remaining < 0 else ''}",
                     f"{remaining}"),
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
                span(class_name="tweet-author", t["author"]),
                p(class_name="tweet-text", t["text"]),
                button(class_name="like-button", on_click=lambda: like(t["id"]),
                       f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
