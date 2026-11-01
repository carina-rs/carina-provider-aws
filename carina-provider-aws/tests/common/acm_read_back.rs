use std::collections::VecDeque;
use std::future::{Future, ready};
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use winterbaume_core::{MockRequest, MockResponse, MockService};

pub const ARN: &str =
    "arn:aws:acm:us-east-1:123456789012:certificate/01234567-89ab-cdef-0123-456789abcdef";

pub struct AcmService {
    pub populated: Mutex<VecDeque<bool>>,
    pub absent: std::sync::atomic::AtomicBool,
    pub requests: AtomicUsize,
    pub describes: AtomicUsize,
    pub mutations: AtomicUsize,
}
impl MockService for AcmService {
    fn service_name(&self) -> &str {
        "acm"
    }
    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://acm\.[^/]+\.amazonaws\.com"]
    }
    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = match request
            .headers
            .get("x-amz-target")
            .unwrap()
            .to_str()
            .unwrap()
        {
            "CertificateManager.RequestCertificate" => {
                self.requests.fetch_add(1, Ordering::Relaxed);
                MockResponse::json(200, format!(r#"{{"CertificateArn":"{ARN}"}}"#))
            }
            "CertificateManager.DescribeCertificate" => {
                self.describes.fetch_add(1, Ordering::Relaxed);
                if self.absent.load(Ordering::Relaxed) {
                    return Box::pin(ready(MockResponse::json(200, "{}")));
                }
                let mut script = self.populated.lock().unwrap();
                let populated = if script.len() > 1 {
                    script.pop_front().unwrap()
                } else {
                    *script.front().unwrap()
                };
                let dvo = if populated {
                    r#", "DomainValidationOptions":[{"DomainName":"example.com","ValidationMethod":"DNS","ResourceRecord":{"Name":"_abc.example.com.","Type":"CNAME","Value":"_xyz.acm-validations.aws."}}]"#
                } else {
                    ""
                };
                MockResponse::json(
                    200,
                    format!(
                        r#"{{"Certificate":{{"CertificateArn":"{ARN}","DomainName":"example.com","Status":"PENDING_VALIDATION"{dvo}}}}}"#
                    ),
                )
            }
            "CertificateManager.ListTagsForCertificate" => {
                MockResponse::json(200, r#"{"Tags":[]}"#)
            }
            "CertificateManager.AddTagsToCertificate"
            | "CertificateManager.RemoveTagsFromCertificate" => {
                self.mutations.fetch_add(1, Ordering::Relaxed);
                MockResponse::json(200, "{}")
            }
            other => panic!("unexpected ACM operation {other}"),
        };
        Box::pin(ready(response))
    }
}
