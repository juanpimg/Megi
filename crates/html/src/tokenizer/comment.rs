//! Comment states (§13.2.5.43–§13.2.5.52).

use super::*;

impl Tokenizer {
    /// Comment start/body/end states.
    pub(super) fn step_comment_states(&mut self) -> bool {
        match self.state {
            // 13.2.5.43 Comment start state
            TokenizerState::CommentStart => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentStartDash;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptClosingOfEmptyComment);
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.44 Comment start dash state
            TokenizerState::CommentStartDash => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentEnd;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptClosingOfEmptyComment);
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push('-');
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.45 Comment state
            TokenizerState::Comment => match self.consume_next() {
                Some(0x3C) => {
                    self.pending_comment.push('<');
                    self.state = TokenizerState::CommentLessThanSign;
                }
                Some(0x2D) => {
                    self.state = TokenizerState::CommentEndDash;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.pending_comment.push('\u{FFFD}');
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    Self::push_cp_to_string(&mut self.pending_comment, cp);
                }
            },

            // 13.2.5.46 Comment less-than sign state
            TokenizerState::CommentLessThanSign => match self.consume_next() {
                Some(0x21) => {
                    self.pending_comment.push('!');
                    self.state = TokenizerState::CommentLessThanSignBang;
                }
                Some(0x3C) => {
                    self.pending_comment.push('<');
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.47 Comment less-than sign bang state
            TokenizerState::CommentLessThanSignBang => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentLessThanSignBangDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.48 Comment less-than sign bang dash state
            TokenizerState::CommentLessThanSignBangDash => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentLessThanSignBangDashDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::CommentEndDash;
                }
            },

            // 13.2.5.49 Comment less-than sign bang dash dash state
            TokenizerState::CommentLessThanSignBangDashDash => match self.consume_next() {
                Some(0x3E) | None => {
                    self.reconsume = true;
                    self.state = TokenizerState::CommentEnd;
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::NestedComment);
                    self.reconsume = true;
                    self.state = TokenizerState::CommentEnd;
                }
            },

            // 13.2.5.50 Comment end dash state
            TokenizerState::CommentEndDash => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentEnd;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push('-');
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.51 Comment end state
            TokenizerState::CommentEnd => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                Some(0x21) => {
                    self.state = TokenizerState::CommentEndBang;
                }
                Some(0x2D) => {
                    self.pending_comment.push('-');
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push_str("--");
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.52 Comment end bang state
            TokenizerState::CommentEndBang => match self.consume_next() {
                Some(0x2D) => {
                    self.pending_comment.push_str("--!");
                    self.state = TokenizerState::CommentEndDash;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::IncorrectlyClosedComment);
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push_str("--!");
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },
            _ => return false,
        }
        true
    }
}
