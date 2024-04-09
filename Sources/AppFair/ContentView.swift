// This is free software: you can redistribute and/or modify it
// under the terms of the GNU General Public License 3.0
// as published by the Free Software Foundation https://fsf.org
import SwiftUI
#if canImport(MarketplaceKitXXX) // TODO: re-enable once com.apple.developer.marketplace.app-installation is granted
import MarketplaceKit
#else
import AppLibrary
#endif

@available(iOS 17.4, *) 
@available(macOS, unavailable)
public struct ContentView: View {
    @AppStorage("setting") var setting = true

    public init() {
    }

    public var body: some View {
        #if SKIP
        EmptyView()
        #else
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
        #endif
    }
}

#if !SKIP
@available(iOS 17.4, macOS 14, *)
@available(macOS, unavailable)
@MainActor public struct AppList: View {
    let library = AppLibrary.current

    @State var viewModel = ViewModel()

    public init() {
    }

    public var body: some View {
        if library.isLoading {
            VStack {
                Text("Loading App Library")
                    .font(.title)
                ProgressView()
            }
        } else {
            NavigationStack {
                Section("Available Apps") {
                    ForEach(library.installedApps.sorted(by: { $0.id < $1.id })) { app in
                        AppLibraryRow(app: app)
                    }
                }
                Section("Installed Apps") {
                    ForEach(library.installedApps.sorted(by: { $0.id < $1.id })) { app in
                        AppLibraryRow(app: app)
                    }
                }
            }
        }
    }

    @ViewBuilder func appDetailView(id: String) -> some View {
        VStack {
            Text("APP DETAIL")
        }
    }
}

@available(iOS 17.4, macOS 14, *)
@available(macOS, unavailable)
public struct AppLibraryRow: View {
    let app: AppLibrary.App

    public var body: some View {
        Text(app.id.description)
    }

}


/// Defines a model that obtains a list of managed apps.
@available(iOS 17.4, macOS 14, *)
@available(macOS, unavailable)
@MainActor @Observable public final class ViewModel {
    let library = AppLibrary.current

//    @Published var content: [ManagedApp] = []
//    @Published var error: Error? = nil
//
//    @MainActor func getApps() async {
//        do {
//            for try await result in ManagedAppLibrary.currentDistributor.availableApps {
//                self.content = try result.get()
//            }
//        } catch {
//            self.error = error
//        }
//    }
}

#endif
