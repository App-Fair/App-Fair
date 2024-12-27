// This is free software: you can redistribute and/or modify it
// under the terms of the GNU General Public License 3.0
// as published by the Free Software Foundation https://fsf.org
import SwiftUI
import AppFairModel

public struct ContentView: View {
    @AppStorage("setting") var setting = true
    @State var viewModel = ViewModel()

    public init() {
    }

    public var body: some View {
        TabView {
            AppList()
                .tabItem { Label("Apps", systemImage: "list.bullet") }

            VStack {
                Text("Welcome to the App Fair!")
                Image(systemName: "heart.fill")
                    .foregroundColor(.red)
            }
            .font(.largeTitle)
            .tabItem { Label("App Fair", systemImage: "house.fill") }

            Form {
                Text("Settings")
                    .font(.largeTitle)
                Toggle("Option", isOn: $setting)
            }
            .tabItem { Label("Settings", systemImage: "gearshape.fill") }
        }
        .environment(AppSource.shared)
        .environment(viewModel)
    }
}

@MainActor public struct AppList: View {
    @Environment(AppSource.self) var source
    @Environment(ViewModel.self) var viewModel

    public init() {
    }

    public var body: some View {
        NavigationStack {
            List {
                ForEach(source.apps) { app in
                    NavigationLink(destination: {
                        AppDetailsView(app: app)
                    }, label: {
                        Label {
                            Text(app.name)
                        } icon: {
                            AsyncImage(url: app.iconURL(version: nil))
                                .frame(width: 29, height: 29)
                        }
                    })
                }
            }
            .navigationTitle(Text("Apps"))
        }
    }
}

struct AppDetailsView : View {
    let app: AppSource.App
    @Environment(AppSource.self) var source
    @Environment(ViewModel.self) var viewModel
    @State var appDescription: String?
    @State var errorMessage: String?

    var body: some View {
        VStack {
            Text(app.name)
                .font(.largeTitle)
            if let appDescription = self.appDescription {
                Text(appDescription)
            }
            if let errorMessage = self.errorMessage {
                Text(errorMessage)
                    .foregroundStyle(.red)
            }
        }
        .navigationTitle(app.name)
        .task {
            await fetchInfo()
        }
    }

    @MainActor func fetchInfo() async {
        do {
            self.appDescription = try await fetch(url: app.descriptionURL(version: nil, locale: nil))
        } catch {
            logger.error("error fetching information for app: \(error)")
            self.errorMessage = error.localizedDescription
        }
    }

    func fetch(url: URL) async throws -> String {
        logger.info("fetching URL: \(url)")
        let (data, response) = try await URLSession.shared.data(from: url)
        if let statusCode = (response as? HTTPURLResponse)?.statusCode {
            guard (200..<300).contains(statusCode) else {
                throw AppError(localizedDescription: "Error fetching info: \(statusCode)")
            }
        }

        guard let str = String(data: data, encoding: .utf8) else {
            throw AppError(localizedDescription: "Error fetching info: response was not a string")
        }

        logger.info("fetched URL: \(url): \(str)")

        return str
    }
}

struct AppError : Error {
    var localizedDescription: String
}
