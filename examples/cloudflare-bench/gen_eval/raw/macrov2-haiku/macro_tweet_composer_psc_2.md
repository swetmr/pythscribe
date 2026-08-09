<!-- exp=macrov2-haiku task=macro_tweet_composer cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.03291530000000001 -->
```python
"use client"
R*

@c
def TweetFeed():
    seed_tweets = [
        {"id": 1, "author": "Alice", "text": "Just launched my new project! Excited to share it with everyone."},
        {"id": 2, "author": "Bob", "text": "Coffee tastes better on Fridays. That's just science."},
        {"id": 3, "author": "Carol", "text": "Anyone else obsessed with organizing their code? Just me?"},
    ]
    
    tweets, set_tweets = us(seed_tweets)
    compose_text, set_compose_text = us("")
    likes, set_likes = us({t["id"]: 0 for t in seed_tweets})
    
    char_count = len(compose_text)
    remaining = 280 - char_count
    can_post = char_count > 0 and char_count <= 280
    
    def post_tweet():
        if can_post:
            new_id = max([t["id"] for t in tweets] or [0]) + 1
            new_tweet = {
                "id": new_id,
                "author": "You",
                "text": compose_text,
            }
            set_tweets([new_tweet, *tweets])
            set_likes({**likes, new_id: 0})
            set_compose_text("")
    
    def toggle_like(tweet_id):
        set_likes({**likes, tweet_id: likes.get(tweet_id, 0) + 1})
    
    def like_handler(tweet_id):
        return lambda: toggle_like(tweet_id)
    
    return div(cn="tweet-feed",
        div(cn="compose-area",
            textarea(
                value=compose_text,
                oh=lambda e: set_compose_text(e.target.value),
                ph="What's happening!?",
                cn="compose-textarea",
                max_length="280",
            ),
            div(cn="compose-footer",
                span(cn="char-counter", f"{remaining} characters remaining"),
                button(
                    oc=lambda: post_tweet(),
                    dis=not can_post,
                    cn="post-button",
                    "Post",
                ),
            ),
        ),
        div(cn="tweets-list",
            *[div(
                key=t["id"],
                cn="tweet",
                div(cn="tweet-header",
                    span(cn="tweet-author", f"@{t['author']}"),
                ),
                div(cn="tweet-text", t["text"]),
                div(cn="tweet-actions",
                    button(
                        oc=like_handler(t["id"]),
                        cn="like-button",
                        f"❤️ {likes.get(t['id'], 0)}",
                    ),
                ),
            ) for t in tweets],
        ),
    )
```
