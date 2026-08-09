<!-- exp=baseline-sonnet task=macro_tweet_composer cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0264123 -->
```python
"use client"

from pyths.react import component, use_state

MAX_LENGTH = 280

INITIAL_TWEETS = [
    {"id": 1, "author": "ada_lovelace", "text": "Just finished analyzing the Analytical Engine's algorithm.", "likes": 42},
    {"id": 2, "author": "grace_hopper", "text": "Found a literal bug in the relay today.", "likes": 128},
    {"id": 3, "author": "alan_turing", "text": "Can machines think? Working on a paper about it.", "likes": 256},
]

def make_tweet(id, author, text):
    return {"id": id, "author": author, "text": text, "likes": 0}

@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")
    next_id, set_next_id = use_state(len(INITIAL_TWEETS) + 1)

    remaining = MAX_LENGTH - len(draft)
    is_empty = len(draft.strip()) == 0
    is_over = remaining < 0
    can_post = not is_empty and not is_over

    def post():
        if not can_post:
            return
        new_tweet = make_tweet(next_id, "you", draft.strip())
        set_tweets([new_tweet, *tweets])
        set_draft("")
        set_next_id(next_id + 1)

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    counter_class = "char-counter over-limit" if is_over else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="compose-box",
            textarea(
                class_name="compose-input",
                value=draft,
                on_change=lambda e: set_draft(e.target.value),
                placeholder="What's happening?",
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
        div(class_name="tweet-list",
            *[
                div(key=t["id"], class_name="tweet",
                    div(class_name="tweet-author", t["author"]),
                    div(class_name="tweet-text", t["text"]),
                    div(class_name="tweet-actions",
                        button(
                            class_name="like-button",
                            on_click=lambda tid=t["id"]: like(tid),
                            f"♥ {t['likes']}",
                        ),
                    ),
                )
                for t in tweets
            ]
        ),
    )
```
