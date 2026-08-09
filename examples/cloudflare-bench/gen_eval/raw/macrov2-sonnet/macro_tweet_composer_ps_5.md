<!-- exp=macrov2-sonnet task=macro_tweet_composer cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.024582 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TWEETS = [
    {"id": 1, "author": "ada_lovelace", "text": "The Analytical Engine weaves algebraic patterns just as the Jacquard loom weaves flowers and leaves.", "likes": 42},
    {"id": 2, "author": "grace_hopper", "text": "The most dangerous phrase in the language is, 'We've always done it this way.'", "likes": 128},
    {"id": 3, "author": "alan_turing", "text": "We can only see a short distance ahead, but we can see plenty there that needs to be done.", "likes": 87},
]

CHAR_LIMIT = 280

def make_tweet(author, text, next_id):
    return {"id": next_id, "author": author, "text": text, "likes": 0}

@component
def TweetFeed():
    tweets, set_tweets = use_state(INITIAL_TWEETS)
    draft, set_draft = use_state("")
    next_id, set_next_id = use_state(1000)

    remaining = CHAR_LIMIT - len(draft)
    over_limit = remaining < 0
    can_post = len(draft.strip()) > 0 and not over_limit

    def post():
        if not can_post:
            return
        new_tweet = make_tweet("you", draft.strip(), next_id)
        set_tweets([new_tweet, *tweets])
        set_draft("")
        set_next_id(next_id + 1)

    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)

    counter_class = "char-counter over-limit" if over_limit else "char-counter"

    return div(class_name="tweet-feed",
        div(class_name="compose",
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
        ul(class_name="tweet-list",
            *[
                li(key=t["id"], class_name="tweet",
                    div(class_name="tweet-author", f"@{t['author']}"),
                    p(class_name="tweet-text", t["text"]),
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
