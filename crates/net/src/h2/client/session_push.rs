//! Server-push bookkeeping and stream completion.

use super::*;

impl<'a, S: Read + Write> Session<'a, S> {
    /// Completes an accepted pushed stream and stores its response (body caps were checked
    /// while receiving DATA).
    pub(super) fn finish_pushed(&mut self, stream_id: u32) {
        let Some(pushed) = self.pushed_streams.remove(&stream_id) else {
            return;
        };
        let Some(status) = pushed.status else {
            self.pushed_discarded += 1;
            return;
        };
        self.pushed_total_bytes = self.pushed_total_bytes.saturating_add(pushed.body.len());
        self.pushed_results.push(H2PushedResponse {
            url: pushed.url,
            status,
            reason: reason_phrase(status),
            headers: pushed.headers,
            body: pushed.body,
            weight: pushed.weight,
            promise_order: pushed.promise_order,
        });
    }

    /// Sorts complete pushes by descending declared weight (FIFO for ties).
    pub(super) fn sort_pushed_results(&mut self) {
        if self.limits.pushed_priority == PushPriority::ServerWeights {
            self.pushed_results.sort_by(|a, b| {
                b.weight
                    .cmp(&a.weight)
                    .then_with(|| a.promise_order.cmp(&b.promise_order))
            });
        }
    }

    /// Cancels a pushed stream (cap exceeded, unwanted, or connection ending).
    pub(super) fn discard_pushed(&mut self, stream_id: u32) {
        self.pushed_streams.remove(&stream_id);
        self.pushed_discarded += 1;
        self.discarded_pushes.insert(stream_id);
        let _ = send_frame(
            &mut *self.stream,
            &Frame::rst_stream(stream_id, error_code::CANCEL),
        );
    }

    pub(super) fn finish_stream(&mut self, stream_id: u32) {
        let Some(stream) = self.streams.remove(&stream_id) else {
            return;
        };
        let Some(status) = stream.status else {
            self.results[stream.request_index] = Some(Err(h2_error(
                "response HEADERS are missing the :status pseudo-header",
            )));
            return;
        };
        self.results[stream.request_index] = Some(Ok(H2Response {
            status,
            reason: reason_phrase(status),
            headers: stream.headers,
            body: stream.body,
            trailers: stream.trailers,
            pushed: Vec::new(),
        }));
    }
}
