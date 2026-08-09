<!-- exp=baseline-sonnet task=macro_tweet_composer cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0231663 -->
```python
"use client"
R*

def make_id():
    return str(math.floor(now() * 1000))

import math

SEED_TWEETS = [
    {"id": "1", "author": "ada_codes", "text": "Shipped a new compiler pass today. Feels good.", "likes": 12},
    {"id": "2", "author": "grace_h", "text": "Debugging is twice as hard as writing the code in the first place.", "likes": 34},
    {"id": "3", "author": "linus_t", "text": "Talk is cheap. Show me the code.", "likes": 88},
]

MAX_LEN = 280

@c
def TweetFeed():
    tweets, set_tweets = us(SEED_TWEETS)
    draft, set_draft = us("")

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": str(len(tweets) + 1) + "-" + draft[:4], "author": "you", "text": draft, "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_draft("")

    def like(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(cn="composer-input", value=draft, ph="What's happening?",
                     oh=lambda e: set_draft(e.target.value)),
            div(cn="composer-footer",
                span(cn="char-counter" if remaining >= 0 else "char-counter over-limit", f"{remaining}"),
                button(cn="post-button", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list", *[
            li(cn="tweet", key=t["id"],
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda tid=t["id"]: like(tid), f"♥ {t['likes']}"),
            ) for t in tweets
        ]),
    )
```
