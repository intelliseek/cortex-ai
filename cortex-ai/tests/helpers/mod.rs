use cortex_ai::FlowError;
use cortex_ai::{
    flow::types::SourceOutput, Condition, ConditionFuture, Flow, FlowComponent, FlowFuture,
    Processor, Sink, Source,
};
use flume::bounded;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::EnvFilter;

// Test Components
#[derive(Debug, Clone)]
pub struct TestProcessor;

#[derive(Clone)]
pub struct TestCondition;

pub struct TestSource {
    pub data: String,
    pub feedback: flume::Sender<Result<String, FlowError>>,
}

#[derive(Clone)]
pub struct PassthroughProcessor;

#[derive(Clone)]
pub struct ErrorProcessor;

pub struct EmptySource;

pub struct StreamErrorSource;

#[derive(Clone)]
pub struct SkipProcessor;

pub struct ErrorSource {
    pub feedback: flume::Sender<Result<String, FlowError>>,
}

pub struct SkipSource {
    pub feedback: flume::Sender<Result<String, FlowError>>,
}

pub struct TestSink;

// Implementations
impl FlowComponent for TestProcessor {
    type Input = String;
    type Output = String;
    type Error = FlowError;
}

impl Processor for TestProcessor {
    fn process(&self, input: Self::Input) -> FlowFuture<'_, Self::Output, Self::Error> {
        Box::pin(async move { Ok(format!("processed_{input}")) })
    }
}

impl FlowComponent for TestCondition {
    type Input = String;
    type Output = String;
    type Error = FlowError;
}

impl Condition for TestCondition {
    fn evaluate(&self, input: Self::Input) -> ConditionFuture<'_, Self::Output, Self::Error> {
        Box::pin(async move {
            let condition_met = input.contains("test");
            Ok((condition_met, Some(input)))
        })
    }
}

impl FlowComponent for TestSource {
    type Input = ();
    type Output = String;
    type Error = FlowError;
}

impl Source for TestSource {
    fn stream(&self) -> FlowFuture<'_, SourceOutput<Self::Output, Self::Error>, Self::Error> {
        let data = self.data.clone();
        let feedback = self.feedback.clone();
        Box::pin(async move {
            let (tx, rx) = bounded(1);
            tx.send(Ok(data)).unwrap();
            drop(tx);

            Ok(SourceOutput {
                receiver: rx,
                feedback,
            })
        })
    }

    fn on_feedback(&self, result: Result<Self::Output, Self::Error>) {
        info!("Source feedback received: {:?}", result);
    }
}

impl FlowComponent for PassthroughProcessor {
    type Input = String;
    type Output = String;
    type Error = FlowError;
}

impl Processor for PassthroughProcessor {
    fn process(&self, input: Self::Input) -> FlowFuture<'_, Self::Output, Self::Error> {
        Box::pin(async move { Ok(input) })
    }
}

impl FlowComponent for ErrorProcessor {
    type Input = String;
    type Output = String;
    type Error = FlowError;
}

impl Processor for ErrorProcessor {
    fn process(&self, _input: Self::Input) -> FlowFuture<'_, Self::Output, Self::Error> {
        Box::pin(async move { Err(FlowError::Process("Processing failed".to_string())) })
    }
}

impl FlowComponent for ErrorSource {
    type Input = ();
    type Output = String;
    type Error = FlowError;
}

impl Source for ErrorSource {
    fn stream(&self) -> FlowFuture<'_, SourceOutput<Self::Output, Self::Error>, Self::Error> {
        let feedback = self.feedback.clone();
        Box::pin(async move {
            let (tx, rx) = bounded(1);
            tx.send(Err(FlowError::Source("Source error".to_string())))
                .unwrap();
            drop(tx);

            Ok(SourceOutput {
                receiver: rx,
                feedback,
            })
        })
    }

    fn on_feedback(&self, result: Result<Self::Output, Self::Error>) {
        info!("Source feedback received: {:?}", result);
    }
}

impl FlowComponent for StreamErrorSource {
    type Input = ();
    type Output = String;
    type Error = FlowError;
}

impl Source for StreamErrorSource {
    fn stream(&self) -> FlowFuture<'_, SourceOutput<Self::Output, Self::Error>, Self::Error> {
        Box::pin(async move { Err(FlowError::Source("Stream initialization error".to_string())) })
    }

    fn on_feedback(&self, result: Result<Self::Output, Self::Error>) {
        info!("Source feedback received: {:?}", result);
    }
}

impl FlowComponent for SkipProcessor {
    type Input = String;
    type Output = String;
    type Error = FlowError;
}

impl Processor for SkipProcessor {
    fn process(&self, _input: Self::Input) -> FlowFuture<'_, Self::Output, Self::Error> {
        Box::pin(async move { Ok("skipped".to_string()) })
    }
}

impl FlowComponent for SkipSource {
    type Input = ();
    type Output = String;
    type Error = FlowError;
}

impl Source for SkipSource {
    fn stream(&self) -> FlowFuture<'_, SourceOutput<Self::Output, Self::Error>, Self::Error> {
        let feedback = self.feedback.clone();
        Box::pin(async move {
            let (tx, rx) = bounded(1);
            tx.send(Ok("to_be_skipped".to_string())).unwrap();
            drop(tx);

            Ok(SourceOutput {
                receiver: rx,
                feedback,
            })
        })
    }

    fn on_feedback(&self, result: Result<Self::Output, Self::Error>) {
        info!("Source feedback received: {:?}", result);
    }
}

impl FlowComponent for EmptySource {
    type Input = ();
    type Output = String;
    type Error = FlowError;
}

impl Source for EmptySource {
    fn stream(&self) -> FlowFuture<'_, SourceOutput<Self::Output, Self::Error>, Self::Error> {
        Box::pin(async move {
            let (tx, rx) = bounded(1);
            let (feedback_tx, feedback_rx) = bounded(1);

            // Don't send any data, just close the channel
            drop(tx);

            // Handle feedback (even though we won't get any)
            tokio::spawn(async move {
                while let Ok(result) = feedback_rx.recv_async().await {
                    match result {
                        Ok(processed_data) => println!("Processing succeeded: {processed_data}"),
                        Err(e) => println!("Processing failed: {e}"),
                    }
                }
            });

            Ok(SourceOutput {
                receiver: rx,
                feedback: feedback_tx,
            })
        })
    }

    fn on_feedback(&self, result: Result<Self::Output, Self::Error>) {
        info!("Source feedback received: {:?}", result);
    }
}

impl FlowComponent for TestSink {
    type Input = String;
    type Output = String;
    type Error = FlowError;
}

impl Sink for TestSink {
    fn sink(&self, input: Self::Input) -> FlowFuture<'_, Self::Output, Self::Error> {
        Box::pin(async move { Ok(input) })
    }
}

// Helper Functions
pub async fn run_flow_with_timeout<DataType, ErrorType, OutputType>(
    flow: Flow<DataType, ErrorType, OutputType>,
    timeout: Duration,
) -> Result<Vec<DataType>, ErrorType>
where
    DataType: Clone + Send + Sync + 'static,
    OutputType: Send + Sync + Clone + 'static,
    ErrorType: Error + Send + Sync + Clone + 'static + From<FlowError>,
{
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let shutdown_tx = Arc::new(shutdown_tx);
    let shutdown_tx_clone = Arc::clone(&shutdown_tx);

    let handle = tokio::spawn(async move {
        let result = flow.run_stream(shutdown_rx).await;
        // Ensure we return before timeout if we have a result
        shutdown_tx_clone.send(()).ok();
        result
    });

    tokio::select! {
        _ = tokio::time::sleep(timeout) => {
            // Send shutdown signal and wait for handle
            shutdown_tx.send(()).ok();
            handle.await.unwrap()
        }
    }
}

// Add this function to initialize tracing for tests
pub fn init_tracing() {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("cortex_ai=debug".parse().unwrap())
                .add_directive("test=debug".parse().unwrap()),
        )
        .with_test_writer() // Write to test output
        .with_thread_ids(true) // Show thread IDs
        .with_file(true) // Show file names
        .with_line_number(true) // Show line numbers
        .with_target(false) // Hide target
        .compact() // Use compact format
        .try_init();

    if subscriber.is_err() {
        println!("Warning: tracing already initialized");
    }
}
