<!-- exp=baseline-001 task=macro_tweet_composer cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.0651105 -->
```python
"use client"

from pyths.react import component, use_state


SEED_TWEETS = [
    {"id": 1, "author": "@ada", "text": "Just shipped a new compiler pass. Feeling good.", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Reminder: test your edge cases before you ship.", "likes": 34},
    {"id": 3, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 128},
]

MAX_LEN = 280


def make_tweet(next_id, text):
    return {"id": next_id, "author": "@you", "text": text, "likes": 0}


@component
def TweetFeed():
    tweets, set_tweets = use_state(SEED_TWEETS)
    draft, set_draft = use_state("")
    next_id, set_next_id = use_state(len(SEED_TWEETS) + 1)

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        set_tweets([make_tweet(next_id, draft.strip()), *tweets])
        set_next_id(next_id + 1)
        set_draft("")

    def like(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])

    counter_class = "char-counter over-limit" if remaining < 0 else "char-counter"

    return div(class_name="tweet-feed",
        h2("Home"),
        div(class_name="composer",
            textarea(
                class_name="composer-input",
                value=draft,
                placeholder="What's happening?",
                on_change=lambda e: set_draft(e.target.value),
            ),
            div(class_name="composer-actions",
                span(class_name=counter_class, f"{remaining}"),
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
                div(class_name="tweet-text", t["text"]),
                button(
                    class_name="like-button",
                    on_click=lambda tid=t["id"]: like(tid),
                    f"♥ {t['likes']}",
                ),
             ) for t in tweets],
        ),
    )
```
