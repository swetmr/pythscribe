<!-- exp=baseline-sonnet task=macro_tweet_composer cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.044499 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TWEETS = [
    {"id": 1, "author": "ada_lovelace", "text": "The Analytical Engine weaves algebraic patterns.", "likes": 42},
    {"id": 2, "author": "grace_hopper", "text": "It's easier to ask forgiveness than permission.", "likes": 128},
    {"id": 3, "author": "linus_t", "text": "Talk is cheap. Show me the code.", "likes": 256},
]

MAX_LEN = 280

def next_id(tweets):
    if not tweets:
        return 1
    return max(t["id"] for t in tweets) + 1

@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {
            "id": next_id(tweets),
            "author": "you",
            "text": draft.strip(),
            "likes": 0,
        }
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    counter_class = "char-counter over-limit" if remaining < 0 else "char-counter"

    return div(class_name="tweet-feed",
        h2("Home"),
        div(class_name="compose-box",
            textarea(
                class_name="compose-input",
                value=draft,
                placeholder="What's happening?",
                on_change=lambda e: set_draft(e.target.value),
            ),
            div(class_name="compose-footer",
                span(class_name=counter_class, f"{remaining}"),
                button(
                    class_name="post-button",
                    disabled=not can_post,
                    on_click=lambda: post(),
                    "Post",
                ),
            ),
        ),
        ul(class_name="tweet-list",
            *[li(key=t["id"], class_name="tweet",
                div(class_name="tweet-author", f"@{t['author']}"),
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
